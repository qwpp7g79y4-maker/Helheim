(function(){
    var TENANT_ID = '{{TENANT_ID}}';
    var TENANT_NAME = '{{TENANT_NAME}}';
    var WELCOME = '{{WELCOME}}';
    var COLOR_PRIMARY = '{{COLOR_PRIMARY}}';
    var COLOR_BG = '{{COLOR_BG}}';
    var COLOR_TEXT = '{{COLOR_TEXT}}';
    var API_BASE = '{{API_BASE}}';
    var messages = [];
    var isOpen = false;
    var isLoading = false;

    // Create styles
    var style = document.createElement('style');
    style.textContent = `
        #hlm-widget-btn {
            position: fixed; bottom: 24px; right: 24px; z-index: 99999;
            width: 60px; height: 60px; border-radius: 50%;
            background: ${COLOR_PRIMARY}; border: none; cursor: pointer;
            box-shadow: 0 4px 20px rgba(0,0,0,0.3);
            display: flex; align-items: center; justify-content: center;
            transition: transform 0.2s, box-shadow 0.2s;
        }
        #hlm-widget-btn:hover { transform: scale(1.1); box-shadow: 0 6px 28px rgba(0,0,0,0.4); }
        #hlm-widget-btn svg { width: 28px; height: 28px; fill: white; }
        #hlm-widget-box {
            position: fixed; bottom: 96px; right: 24px; z-index: 99999;
            width: 380px; max-width: calc(100vw - 48px); height: 520px; max-height: calc(100vh - 120px);
            background: ${COLOR_BG}; border: 1px solid rgba(255,255,255,0.1);
            border-radius: 16px; box-shadow: 0 8px 40px rgba(0,0,0,0.5);
            display: none; flex-direction: column; overflow: hidden;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        }
        #hlm-widget-box.open { display: flex; }
        #hlm-header {
            padding: 16px 20px; background: ${COLOR_PRIMARY};
            color: white; font-weight: 600; font-size: 15px;
            display: flex; align-items: center; justify-content: space-between;
        }
        #hlm-header button {
            background: none; border: none; color: white; cursor: pointer;
            font-size: 20px; padding: 0; line-height: 1;
        }
        #hlm-messages {
            flex: 1; overflow-y: auto; padding: 16px;
            display: flex; flex-direction: column; gap: 10px;
        }
        .hlm-msg {
            max-width: 85%; padding: 10px 14px; border-radius: 12px;
            font-size: 14px; line-height: 1.5; color: ${COLOR_TEXT};
            word-wrap: break-word;
        }
        .hlm-msg-bot {
            align-self: flex-start;
            background: rgba(255,255,255,0.08);
            border: 1px solid rgba(255,255,255,0.1);
        }
        .hlm-msg-user {
            align-self: flex-end;
            background: ${COLOR_PRIMARY};
            color: white;
        }
        .hlm-typing {
            align-self: flex-start; padding: 10px 14px;
            background: rgba(255,255,255,0.08); border-radius: 12px;
            font-size: 14px; color: rgba(255,255,255,0.5);
        }
        .hlm-typing span { animation: hlm-blink 1.4s infinite both; }
        .hlm-typing span:nth-child(2) { animation-delay: 0.2s; }
        .hlm-typing span:nth-child(3) { animation-delay: 0.4s; }
        @keyframes hlm-blink { 0%,80%,100%{opacity:0.3} 40%{opacity:1} }
        #hlm-input-area {
            padding: 12px 16px; border-top: 1px solid rgba(255,255,255,0.1);
            display: flex; gap: 8px;
        }
        #hlm-input {
            flex: 1; background: rgba(255,255,255,0.08); border: 1px solid rgba(255,255,255,0.15);
            border-radius: 8px; padding: 10px 14px; color: ${COLOR_TEXT};
            font-size: 14px; outline: none; resize: none;
        }
        #hlm-input::placeholder { color: rgba(255,255,255,0.35); }
        #hlm-input:focus { border-color: ${COLOR_PRIMARY}; }
        #hlm-send {
            background: ${COLOR_PRIMARY}; border: none; border-radius: 8px;
            padding: 10px 16px; cursor: pointer; color: white; font-size: 14px;
            font-weight: 600; white-space: nowrap;
        }
        #hlm-send:disabled { opacity: 0.5; cursor: not-allowed; }
        #hlm-powered {
            text-align: center; padding: 6px; font-size: 11px;
            color: rgba(255,255,255,0.25);
        }
        #hlm-powered a { color: rgba(255,255,255,0.35); text-decoration: none; }
    `;
    document.head.appendChild(style);

    // Create button
    var btn = document.createElement('button');
    btn.id = 'hlm-widget-btn';
    btn.innerHTML = '<svg viewBox="0 0 24 24"><path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H6l-2 2V4h16v12z"/></svg>';
    btn.onclick = function() { toggle(); };
    document.body.appendChild(btn);

    // Create chat box
    var box = document.createElement('div');
    box.id = 'hlm-widget-box';
    box.innerHTML = `
        <div id="hlm-header">
            <span>${TENANT_NAME}</span>
            <button onclick="document.getElementById('hlm-widget-box').classList.remove('open')">&times;</button>
        </div>
        <div id="hlm-messages"></div>
        <div id="hlm-input-area">
            <input id="hlm-input" placeholder="Stel een vraag..." autocomplete="off">
            <button id="hlm-send">Verstuur</button>
        </div>
        <div id="hlm-powered">Powered by <a href="https://helheim-ai.dev" target="_blank">Helheim AI</a></div>
    `;
    document.body.appendChild(box);

    // Add welcome message
    addMsg('bot', WELCOME);

    // Events
    document.getElementById('hlm-send').onclick = send;
    document.getElementById('hlm-input').onkeydown = function(e) {
        if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); }
    };

    function toggle() {
        isOpen = !isOpen;
        box.classList.toggle('open', isOpen);
        if (isOpen) document.getElementById('hlm-input').focus();
    }

    function addMsg(type, text) {
        var el = document.createElement('div');
        el.className = 'hlm-msg hlm-msg-' + (type === 'bot' ? 'bot' : 'user');
        el.textContent = text;
        var container = document.getElementById('hlm-messages');
        container.appendChild(el);
        container.scrollTop = container.scrollHeight;
        return el;
    }

    function showTyping() {
        var el = document.createElement('div');
        el.className = 'hlm-typing';
        el.id = 'hlm-typing';
        el.innerHTML = '<span>●</span> <span>●</span> <span>●</span>';
        var container = document.getElementById('hlm-messages');
        container.appendChild(el);
        container.scrollTop = container.scrollHeight;
    }

    function hideTyping() {
        var el = document.getElementById('hlm-typing');
        if (el) el.remove();
    }

    function send() {
        if (isLoading) return;
        var input = document.getElementById('hlm-input');
        var text = input.value.trim();
        if (!text) return;
        input.value = '';

        // Add user message
        messages.push({ role: 'user', content: text });
        addMsg('user', text);

        // Show typing indicator
        isLoading = true;
        document.getElementById('hlm-send').disabled = true;
        showTyping();

        // Send to API
        fetch(API_BASE + '/api/v1/widget/' + TENANT_ID + '/chat', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ messages: messages })
        })
        .then(function(r) { return r.json(); })
        .then(function(data) {
            hideTyping();
            isLoading = false;
            document.getElementById('hlm-send').disabled = false;
            if (data.response) {
                messages.push({ role: 'assistant', content: data.response });
                addMsg('bot', data.response);
            } else if (data.error) {
                addMsg('bot', 'Sorry, er ging iets mis. Probeer het later opnieuw.');
            }
        })
        .catch(function() {
            hideTyping();
            isLoading = false;
            document.getElementById('hlm-send').disabled = false;
            addMsg('bot', 'Sorry, ik kan even niet bereikt worden. Probeer het later opnieuw.');
        });
    }
})();
