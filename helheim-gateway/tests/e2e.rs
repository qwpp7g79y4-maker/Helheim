use helheim_protocol::*;
use helheim_taskqueue::TaskQueue;
use std::sync::Arc;
use axum::{routing::{get, post}, extract::{State, Json, Path}, http::StatusCode, Router};

struct AppState {
    queue: TaskQueue,
    keys: Vec<String>,
}

#[tokio::test]
async fn full_e2e_flow() {
    // 1. Setup in-process gateway
    let state = Arc::new(AppState {
        queue: TaskQueue::new(),
        keys: vec!["test-key-123".to_string()],
    });

    let app = Router::new()
        .route("/health", get(|| async { Json(serde_json::json!({"status": "ok"})) }))
        .route("/api/v1/tasks", post({
            let s = state.clone();
            move |Json(req): Json<ApiRequest>| {
                let s = s.clone();
                async move {
                    match s.queue.submit(req.api_key, req.task, req.priority).await {
                        Ok(task_id) => Json(serde_json::json!({"task_id": task_id, "status": "Queued"})),
                        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
                    }
                }
            }
        }))
        .route("/api/v1/tasks/:task_id", get({
            let s = state.clone();
            move |Path(task_id): Path<String>| {
                let s = s.clone();
                async move {
                    match s.queue.get_task(&task_id).await {
                        Some(task) => Json(serde_json::json!({
                            "task_id": task.id,
                            "status": format!("{:?}", task.status),
                            "result": task.result,
                        })),
                        None => Json(serde_json::json!({"error": "not found"})),
                    }
                }
            }
        }))
        .route("/api/v1/nodes/register", post({
            let s = state.clone();
            move |Json(caps): Json<NodeCapabilities>| {
                let s = s.clone();
                async move {
                    let id = caps.node_id.clone();
                    s.queue.register_node(caps).await;
                    s.queue.try_assign().await;
                    Json(serde_json::json!({"status": "registered", "node_id": id}))
                }
            }
        }))
        .route("/api/v1/node/:node_id/tasks", get({
            let s = state.clone();
            move |Path(node_id): Path<String>| {
                let s = s.clone();
                async move { Json(s.queue.get_assigned_tasks(&node_id).await) }
            }
        }))
        .route("/api/v1/task/:task_id/complete", post({
            let s = state.clone();
            move |Path(task_id): Path<String>, Json(result): Json<TaskResult>| {
                let s = s.clone();
                async move {
                    s.queue.complete_task(&task_id, result).await;
                    Json(serde_json::json!({"status": "completed"}))
                }
            }
        }))
        .route("/api/v1/stats", get({
            let s = state.clone();
            move || {
                let s = s.clone();
                async move { Json(s.queue.stats().await) }
            }
        }));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{}", addr);
    
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // 2. Health check
    let resp = client.get(format!("{}/health", base)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    println!("✅ Health check passed");

    // 3. Register a node
    let node = NodeCapabilities {
        node_id: "test-node".to_string(),
        has_cuda: true,
        cpu_cores: 32,
        estimated_cpu_gflops: 150.0,
        ram_mb: 131072,
        gpu_count: 2,
        gpu_models: vec!["RTX 5060".into(), "RTX 3060".into()],
        total_vram_mb: 28672,
        disk_free_mb: 500000,
        public_ip: None,
        capabilities: vec![
            Capability::GpuCompute, Capability::GpuInference,
            Capability::HeavyCpu, Capability::Hashing, Capability::LogAnalysis,
        ],
    };
    let resp = client.post(format!("{}/api/v1/nodes/register", base))
        .json(&node).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["node_id"], "test-node");
    println!("✅ Node registered: {}", body);

    // 4. Submit a task
    let req = ApiRequest {
        api_key: "test-key-123".to_string(),
        task: TaskType::GpuMatMul { size: 256 },
        priority: Some(Priority::High),
    };
    let resp = client.post(format!("{}/api/v1/tasks", base))
        .json(&req).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let task_id = body["task_id"].as_str().unwrap().to_string();
    println!("✅ Task submitted: {}", task_id);

    // 5. Check task was assigned
    let resp = client.get(format!("{}/api/v1/tasks/{}", base, task_id))
        .send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "Assigned");
    println!("✅ Task assigned: {}", body["status"]);

    // 6. Node polls for tasks
    let resp = client.get(format!("{}/api/v1/node/test-node/tasks", base))
        .send().await.unwrap();
    let tasks: Vec<Task> = resp.json().await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task_id);
    println!("✅ Node received task: {:?}", tasks[0].task_type);

    // 7. Node reports completion
    let result = TaskResult {
        success: true,
        output: "MatMul 256x256 completed in 42ms (123.45 GFLOPS)".to_string(),
        duration_ms: 42,
        error: None,
    };
    let resp = client.post(format!("{}/api/v1/task/{}/complete", base, task_id))
        .json(&result).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    println!("✅ Task completion reported");

    // 8. Verify task is completed
    let resp = client.get(format!("{}/api/v1/tasks/{}", base, task_id))
        .send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "Completed");
    let result_output = body["result"]["output"].as_str().unwrap();
    assert!(result_output.contains("GFLOPS"));
    println!("✅ Task completed with result: {}", result_output);

    // 9. Check stats
    let resp = client.get(format!("{}/api/v1/stats", base)).send().await.unwrap();
    let stats: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(stats["completed"], 1);
    assert_eq!(stats["nodes_online"], 1);
    println!("✅ Stats: {}", stats);

    println!("\n🏁 ALL E2E TESTS PASSED");
}
