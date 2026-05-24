use axum::{
    extract::{State, Json, Query},
    http::{StatusCode, HeaderMap},
    response::Html,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::AppState;

const DEVELOPER_PRICE_ID: &str = "price_1T1YYV3GsjPycIr8lCdUtTpF";
const BUSINESS_PRICE_ID: &str = "price_1T1YYx3GsjPycIr8r565JYZL";

const DEVELOPER_CREDITS: i64 = 5000;
const BUSINESS_CREDITS: i64 = 25000;

fn stripe_sk() -> String {
    std::env::var("STRIPE_SK").unwrap_or_default()
}

fn _stripe_pk() -> String {
    std::env::var("STRIPE_PK").unwrap_or_default()
}

// --- Checkout Session ---

#[derive(Deserialize)]
pub struct CheckoutRequest {
    pub plan: String, // "developer" or "business"
}

pub async fn create_checkout(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let price_id = match req.plan.as_str() {
        "developer" => DEVELOPER_PRICE_ID,
        "business" => BUSINESS_PRICE_ID,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let sk = stripe_sk();
    if sk.is_empty() {
        tracing::error!("[STRIPE] STRIPE_SK not set");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let client = reqwest::Client::new();
    let res = client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(&sk, None::<&str>)
        .form(&[
            ("mode", "subscription"),
            ("payment_method_types[]", "card"),
            ("line_items[0][price]", price_id),
            ("line_items[0][quantity]", "1"),
            ("success_url", "https://helheim-ai.dev/api/v1/checkout/success?session_id={CHECKOUT_SESSION_ID}"),
            ("cancel_url", "https://helheim-ai.dev/#pricing"),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("[STRIPE] Request failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let body: serde_json::Value = res.json().await.map_err(|e| {
        tracing::error!("[STRIPE] Parse failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(url) = body["url"].as_str() {
        info!("[STRIPE] Checkout session created: {}", body["id"]);
        Ok(Json(serde_json::json!({ "url": url, "session_id": body["id"] })))
    } else {
        tracing::error!("[STRIPE] No URL in response: {:?}", body);
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

// --- Checkout Success ---

#[derive(Deserialize)]
pub struct SuccessParams {
    pub session_id: String,
}

pub async fn checkout_success(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SuccessParams>,
) -> Result<Html<String>, StatusCode> {
    let sk = stripe_sk();

    // Fetch session from Stripe to get customer email + plan info
    let client = reqwest::Client::new();
    let session_url = format!(
        "https://api.stripe.com/v1/checkout/sessions/{}?expand[]=line_items",
        params.session_id
    );

    let session_body = client
        .get(&session_url)
        .basic_auth(&sk, None::<&str>)
        .send()
        .await
        .map_err(|e| { tracing::error!("[STRIPE] Session fetch failed: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| { tracing::error!("[STRIPE] Session parse failed: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;

    let customer_email = session_body["customer_details"]["email"]
        .as_str()
        .unwrap_or("customer")
        .to_string()
        .trim()
        .to_lowercase();

    let stripe_customer_id = session_body["customer"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Determine plan from the price ID in the session
    let price_id = session_body["line_items"]["data"][0]["price"]["id"]
        .as_str()
        .unwrap_or("");
    let (plan, credits) = match price_id {
        p if p == DEVELOPER_PRICE_ID => ("developer", DEVELOPER_CREDITS),
        p if p == BUSINESS_PRICE_ID => ("business", BUSINESS_CREDITS),
        _ => {
            // Fallback: check subscription metadata or default to developer
            info!("[STRIPE] Unknown price_id '{}', defaulting to developer", price_id);
            ("developer", DEVELOPER_CREDITS)
        }
    };

    // Find existing user by email, or create a new one
    let api_key = if let Some(existing_key) = state.api_keys.get_api_key_for_email(&customer_email) {
        info!("[STRIPE] Found existing user for {}: {}", customer_email, existing_key);
        existing_key
    } else {
        // No existing user — create one via get_or_create_key_sync
        let key = state.api_keys.get_or_create_key_sync(&format!("stripe-{}", customer_email));
        info!("[STRIPE] Created new user for {}: {}", customer_email, key);
        key
    };

    // Set plan, credits, and stripe customer ID
    state.api_keys.set_plan(&api_key, plan);
    if !stripe_customer_id.is_empty() {
        state.api_keys.set_stripe_customer_id(&api_key, &stripe_customer_id);
    }
    let _ = state.api_keys.adjust_credits(&api_key, credits,
        &format!("stripe_checkout:{}", plan), None, Some(&params.session_id));

    let new_balance = state.api_keys.get_credits(&api_key);
    info!(
        "[STRIPE] Payment success! {} -> plan={}, +{} credits (balance={}), key={}",
        customer_email, plan, credits, new_balance, api_key
    );

    Ok(Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Payment Successful - Helheim AI</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <style>
        body {{ background: #06060b; color: #e2e8f0; font-family: 'Inter', sans-serif; }}
        .card {{ background: rgba(12, 12, 20, 0.9); border: 1px solid rgba(99, 102, 241, 0.3); }}
    </style>
</head>
<body class="min-h-screen flex items-center justify-center">
    <div class="card rounded-2xl p-10 max-w-lg text-center">
        <div class="text-5xl mb-6">&#10003;</div>
        <h1 class="text-3xl font-bold mb-4 text-green-400">Payment Successful!</h1>
        <p class="text-gray-400 mb-6">Welcome to Helheim AI, <strong>{email}</strong>.</p>
        
        <div class="bg-black/50 rounded-xl p-6 mb-6 text-left">
            <p class="text-sm text-gray-400 mb-2">Your API Key:</p>
            <div class="flex items-center gap-2">
                <code class="text-indigo-300 text-sm break-all flex-1" id="key">{key}</code>
                <button onclick="navigator.clipboard.writeText(document.getElementById('key').textContent)" 
                        class="text-xs bg-indigo-600/20 text-indigo-400 px-3 py-1 rounded hover:bg-indigo-600/30">
                    Copy
                </button>
            </div>
        </div>
        
        <p class="text-sm text-gray-500 mb-6">Save this key! You'll need it for API requests.</p>
        
        <div class="bg-black/50 rounded-xl p-4 text-left text-sm mb-6" style="font-family: monospace;">
            <span style="color: #ff7b72;">from</span> openai <span style="color: #ff7b72;">import</span> OpenAI<br><br>
            client = OpenAI(<br>
            &nbsp;&nbsp;<span style="color: #ffa657;">base_url</span>=<span style="color: #a5d6ff;">"https://api.helheim-ai.dev/v1"</span>,<br>
            &nbsp;&nbsp;<span style="color: #ffa657;">api_key</span>=<span style="color: #a5d6ff;">"{key}"</span><br>
            )
        </div>
        
        <a href="/dashboard?key={key}" class="inline-block bg-gradient-to-r from-indigo-500 to-purple-600 text-white px-8 py-3 rounded-xl font-semibold hover:opacity-90">
            Go to Dashboard
        </a>
    </div>
</body>
</html>"#,
        email = customer_email,
        key = api_key,
    )))
}

// --- Stripe Webhook ---

pub async fn stripe_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Verify webhook signature if STRIPE_WEBHOOK_SECRET is set
    if let Ok(webhook_secret) = std::env::var("STRIPE_WEBHOOK_SECRET") {
        let sig_header = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !verify_stripe_signature(&body, sig_header, &webhook_secret) {
            tracing::warn!("[STRIPE] Invalid webhook signature");
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let event: serde_json::Value = serde_json::from_str(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    let event_type = event["type"].as_str().unwrap_or("");
    info!("[STRIPE] Webhook event: {}", event_type);

    match event_type {
        "checkout.session.completed" => {
            let email = event["data"]["object"]["customer_details"]["email"]
                .as_str()
                .unwrap_or("webhook-customer")
                .trim()
                .to_lowercase();
            let customer_id = event["data"]["object"]["customer"]
                .as_str()
                .unwrap_or("");

            // Find existing user or create new
            let api_key = if let Some(key) = state.api_keys.get_api_key_for_email(&email) {
                key
            } else {
                state.api_keys.get_or_create_key_sync(&format!("stripe-{}", email))
            };

            // Determine plan from subscription
            let price_id = event["data"]["object"]["line_items"]["data"][0]["price"]["id"]
                .as_str()
                .unwrap_or("");
            let (plan, credits) = match price_id {
                p if p == DEVELOPER_PRICE_ID => ("developer", DEVELOPER_CREDITS),
                p if p == BUSINESS_PRICE_ID => ("business", BUSINESS_CREDITS),
                _ => ("developer", DEVELOPER_CREDITS),
            };

            state.api_keys.set_plan(&api_key, plan);
            if !customer_id.is_empty() {
                state.api_keys.set_stripe_customer_id(&api_key, customer_id);
            }
            let _ = state.api_keys.adjust_credits(&api_key, credits,
                &format!("stripe_webhook:checkout:{}", plan), None, None);

            info!("[STRIPE] Webhook: checkout completed for {} -> plan={}, +{} credits", email, plan, credits);
        }
        "invoice.payment_succeeded" => {
            // Monthly recurring payment — add credits
            let customer_id = event["data"]["object"]["customer"]
                .as_str()
                .unwrap_or("");
            let customer_email = event["data"]["object"]["customer_email"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_lowercase();

            if let Some(api_key) = state.api_keys.get_api_key_for_email(&customer_email) {
                let plan = state.api_keys.get_plan(&api_key);
                let credits = match plan.as_str() {
                    "business" => BUSINESS_CREDITS,
                    _ => DEVELOPER_CREDITS,
                };
                let _ = state.api_keys.adjust_credits(&api_key, credits,
                    &format!("stripe_webhook:invoice:{}", plan), None, None);
                info!("[STRIPE] Webhook: invoice paid for {} -> +{} credits", customer_email, credits);
            } else {
                info!("[STRIPE] Webhook: invoice paid for unknown customer {} ({})", customer_email, customer_id);
            }
        }
        "customer.subscription.deleted" => {
            let customer_email = event["data"]["object"]["customer_email"]
                .as_str()
                .or_else(|| event["data"]["object"]["metadata"]["email"].as_str())
                .unwrap_or("")
                .trim()
                .to_lowercase();

            if let Some(api_key) = state.api_keys.get_api_key_for_email(&customer_email) {
                state.api_keys.set_plan(&api_key, "free");
                info!("[STRIPE] Webhook: subscription cancelled for {} -> plan=free", customer_email);
            } else {
                info!("[STRIPE] Webhook: subscription cancelled for unknown email {}", customer_email);
            }
        }
        _ => {
            info!("[STRIPE] Unhandled event: {}", event_type);
        }
    }

    Ok(Json(serde_json::json!({ "received": true })))
}

fn verify_stripe_signature(payload: &str, sig_header: &str, secret: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    // Parse timestamp and signature from header
    let mut timestamp = "";
    let mut signature = "";
    for part in sig_header.split(',') {
        let part = part.trim();
        if let Some(t) = part.strip_prefix("t=") {
            timestamp = t;
        } else if let Some(v) = part.strip_prefix("v1=") {
            signature = v;
        }
    }

    if timestamp.is_empty() || signature.is_empty() {
        return false;
    }

    let signed_payload = format!("{}.{}", timestamp, payload);
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(signed_payload.as_bytes());

    let expected = hex::encode(mac.finalize().into_bytes());
    expected == signature
}
