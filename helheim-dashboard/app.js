// Helheim Starfield SNN Frontend Logic
const API_URL = 'http://localhost:8080/api/execute';
const WS_URL = 'ws://localhost:8080/ws/spikes';

// UI Elements
const editor = document.getElementById('code-editor');
const btnExecute = document.getElementById('btn-execute');
const terminal = document.getElementById('terminal');
const wsStatusDot = document.getElementById('ws-status');
const wsStatusText = document.getElementById('ws-text');
const canvas = document.getElementById('starfield-canvas');
const ctx = canvas.getContext('2d');

// State
let socket;
let neurons = [];
let spikeQueue = [];

// Canvas Setup
function resizeCanvas() {
    canvas.width = canvas.parentElement.clientWidth;
    canvas.height = canvas.parentElement.clientHeight;
    initNeurons();
}

window.addEventListener('resize', resizeCanvas);

// Neuron Visualization
class Neuron {
    constructor(x, y, radius) {
        this.baseX = x;
        this.baseY = y;
        this.x = x;
        this.y = y;
        this.radius = radius;
        this.isFiring = false;
        this.glowIntensity = 0;
        this.vx = (Math.random() - 0.5) * 0.2;
        this.vy = (Math.random() - 0.5) * 0.2;
    }

    fire() {
        this.isFiring = true;
        this.glowIntensity = 1.0;
    }

    update() {
        // Slow drift
        this.x += this.vx;
        this.y += this.vy;
        
        // Return to base
        if (Math.abs(this.x - this.baseX) > 20) this.vx *= -1;
        if (Math.abs(this.y - this.baseY) > 20) this.vy *= -1;

        // Fade out
        if (this.glowIntensity > 0) {
            this.glowIntensity -= 0.02;
            if (this.glowIntensity <= 0) {
                this.glowIntensity = 0;
                this.isFiring = false;
            }
        }
    }

    draw(ctx) {
        ctx.beginPath();
        ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2);
        
        if (this.isFiring) {
            ctx.fillStyle = '#00ffcc';
            ctx.shadowBlur = 20 * this.glowIntensity;
            ctx.shadowColor = '#00ffcc';
        } else {
            ctx.fillStyle = 'rgba(255, 255, 255, 0.1)';
            ctx.shadowBlur = 0;
        }
        
        ctx.fill();
        ctx.closePath();
    }
}

function initNeurons() {
    neurons = [];
    const cols = 8;
    const rows = 6;
    const spacingX = canvas.width / (cols + 1);
    const spacingY = canvas.height / (rows + 1);

    for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
            const x = spacingX * (c + 1) + (Math.random() * 20 - 10);
            const y = spacingY * (r + 1) + (Math.random() * 20 - 10);
            neurons.push(new Neuron(x, y, 4));
        }
    }
}

function drawConnections() {
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.03)';
    ctx.lineWidth = 1;
    
    for (let i = 0; i < neurons.length; i++) {
        for (let j = i + 1; j < neurons.length; j++) {
            const dx = neurons[i].x - neurons[j].x;
            const dy = neurons[i].y - neurons[j].y;
            const dist = Math.sqrt(dx*dx + dy*dy);
            
            if (dist < 150) {
                ctx.moveTo(neurons[i].x, neurons[i].y);
                ctx.lineTo(neurons[j].x, neurons[j].y);
            }
        }
    }
    ctx.stroke();
    ctx.closePath();
}

function animate() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    
    drawConnections();
    
    neurons.forEach(n => {
        n.update();
        n.draw(ctx);
    });
    
    requestAnimationFrame(animate);
}

// Process incoming spikes
function triggerSpikes(spikeArray) {
    // Flatten 2D arrays if necessary
    const flatSpikes = spikeArray.flat(Infinity);
    
    // Pick random neurons to fire based on the spike data
    let spikeIndex = 0;
    
    // Create a shuffled array of indices
    const indices = Array.from({length: neurons.length}, (_, i) => i);
    for (let i = indices.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [indices[i], indices[j]] = [indices[j], indices[i]];
    }

    flatSpikes.forEach(val => {
        if ((val === "waar" || val === true) && spikeIndex < indices.length) {
            neurons[indices[spikeIndex]].fire();
            spikeIndex++;
        }
    });
}

// Logging
function logToTerminal(msg) {
    const timestamp = new Date().toLocaleTimeString();
    terminal.innerHTML += `\n[${timestamp}] ${msg}`;
    terminal.scrollTop = terminal.scrollHeight;
}

// API Execution
async function executeScript() {
    const script = editor.value;
    if (!script.trim()) return;

    btnExecute.disabled = true;
    btnExecute.textContent = "Executing...";
    logToTerminal("Dispatching PTX Kernel...");

    try {
        const response = await fetch(API_URL, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ script: script })
        });
        
        const data = await response.json();
        
        if (data.status === 'success') {
            logToTerminal(`Execution Success:\nResult: ${data.result}`);
            if (data.spikes) {
                logToTerminal(`Received ${data.spikes.length} spikes.`);
                triggerSpikes(data.spikes);
            }
        } else {
            logToTerminal(`Error: ${data.message || 'Unknown execution error'}`);
        }
    } catch (error) {
        logToTerminal(`Network Error: ${error.message}`);
    } finally {
        btnExecute.disabled = false;
        btnExecute.textContent = "Execute PTX Kernel";
    }
}

// WebSocket Connection
function connectWebSocket() {
    socket = new WebSocket(WS_URL);

    socket.onopen = () => {
        wsStatusDot.className = 'dot connected';
        wsStatusText.textContent = 'WebSocket: Connected';
        logToTerminal('HSP Swarm Stream Active.');
    };

    socket.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            if (msg.type === 'spikes' && msg.data) {
                logToTerminal(`[WS] Real-time Spikes Detected`);
                triggerSpikes(msg.data);
            }
        } catch(e) {
            console.error("WS Parse Error", e);
        }
    };

    socket.onclose = () => {
        wsStatusDot.className = 'dot disconnected';
        wsStatusText.textContent = 'WebSocket: Disconnected';
        // Auto-reconnect
        setTimeout(connectWebSocket, 5000);
    };
    
    socket.onerror = (err) => {
        console.error('WebSocket Error', err);
    };
}

// Init
btnExecute.addEventListener('click', executeScript);

// Boot
setTimeout(() => {
    resizeCanvas();
    animate();
    connectWebSocket();
    logToTerminal('Starfield Visualizer Initialized.');
}, 100);
