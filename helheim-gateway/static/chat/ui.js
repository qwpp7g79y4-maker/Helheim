/* === Helheim Chat — UI Module === */

var ChatUI = {
    welcomeHTML: '<div class="flex justify-center py-12" id="welcome-screen">'
        + '<div class="text-center max-w-lg">'
        + '<div class="text-5xl mb-4">⚡</div>'
        + '<h2 class="text-xl font-bold text-gray-200 mb-2">Helheim Chat</h2>'
        + '<p class="text-sm text-gray-500 mb-6">Powered by your distributed GPU cluster.<br>Select a model and start chatting.</p>'
        + '<div class="flex flex-wrap gap-2 justify-center">'
        + '<button onclick="ChatActions.setPrompt(\'Explain quantum computing in simple terms\')" class="text-xs bg-indigo-600/10 text-indigo-400 border border-indigo-600/20 px-3 py-1.5 rounded-lg hover:bg-indigo-600/20 cursor-pointer">Quantum computing</button>'
        + '<button onclick="ChatActions.setPrompt(\'Write a Python quicksort function\')" class="text-xs bg-indigo-600/10 text-indigo-400 border border-indigo-600/20 px-3 py-1.5 rounded-lg hover:bg-indigo-600/20 cursor-pointer">Python quicksort</button>'
        + '<button onclick="ChatActions.setPrompt(\'Wat is het verschil tussen TCP en UDP?\')" class="text-xs bg-indigo-600/10 text-indigo-400 border border-indigo-600/20 px-3 py-1.5 rounded-lg hover:bg-indigo-600/20 cursor-pointer">TCP vs UDP</button>'
        + '<button onclick="ChatActions.setPrompt(\'Write a Rust async web server with Axum\')" class="text-xs bg-purple-600/10 text-purple-400 border border-purple-600/20 px-3 py-1.5 rounded-lg hover:bg-purple-600/20 cursor-pointer">Rust Axum server</button>'
        + '</div></div></div>',

    scrollToBottom: function() {
        var el = document.getElementById('chat-messages');
        if (el) el.scrollTop = el.scrollHeight;
    },

    escapeHtml: function(text) {
        var div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    },

    renderMarkdown: function(text) {
        var html = this.escapeHtml(text);

        // Code blocks with language
        html = html.replace(/```(\w+)\n([\s\S]*?)```/g, function(match, lang, code) {
            return '<div class="code-block-wrapper"><span class="code-lang">' + lang + '</span>'
                + '<button class="copy-btn" onclick="ChatUI.copyCode(this)">Copy</button>'
                + '<pre><code class="language-' + lang + '">' + code.trim() + '</code></pre></div>';
        });
        // Code blocks without language
        html = html.replace(/```([\s\S]*?)```/g, function(match, code) {
            return '<div class="code-block-wrapper">'
                + '<button class="copy-btn" onclick="ChatUI.copyCode(this)">Copy</button>'
                + '<pre><code>' + code.trim() + '</code></pre></div>';
        });
        // Inline code
        html = html.replace(/`([^`]+)`/g, '<code>$1</code>');
        // Bold
        html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
        // Italic
        html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');
        // Headers
        html = html.replace(/^### (.+)$/gm, '<h3>$1</h3>');
        html = html.replace(/^## (.+)$/gm, '<h2>$1</h2>');
        html = html.replace(/^# (.+)$/gm, '<h1>$1</h1>');
        // Lists
        html = html.replace(/^\* (.+)$/gm, '<li>$1</li>');
        html = html.replace(/^- (.+)$/gm, '<li>$1</li>');
        html = html.replace(/^\d+\. (.+)$/gm, '<li>$1</li>');
        // Blockquotes
        html = html.replace(/^&gt; (.+)$/gm, '<blockquote>$1</blockquote>');
        // Line breaks
        html = html.replace(/\n/g, '<br>');
        // Clean up breaks around block elements
        html = html.replace(/<br><(h[123]|pre|blockquote|li|div)/g, '<$1');
        html = html.replace(/<\/(h[123]|pre|blockquote|li|div)><br>/g, '</$1>');
        return html;
    },

    copyCode: function(btn) {
        var pre = btn.parentElement.querySelector('pre code');
        if (!pre) return;
        navigator.clipboard.writeText(pre.textContent).then(function() {
            btn.textContent = 'Copied!';
            btn.classList.add('copied');
            setTimeout(function() {
                btn.textContent = 'Copy';
                btn.classList.remove('copied');
            }, 2000);
        });
    },

    removeWelcome: function() {
        var w = document.getElementById('welcome-screen');
        if (w) w.remove();
    },

    addMessage: function(role, content, isStreaming) {
        this.removeWelcome();
        var container = document.getElementById('chat-messages');

        var wrapper = document.createElement('div');
        wrapper.className = 'msg-wrapper flex ' + (role === 'user' ? 'justify-end' : 'justify-start');

        var col = document.createElement('div');
        col.className = 'max-w-[80%] flex flex-col gap-1';

        var bubble = document.createElement('div');
        bubble.className = 'rounded-2xl px-4 py-3 ' + (role === 'user' ? 'msg-user' : 'msg-assistant');

        var header = document.createElement('div');
        header.className = 'flex items-center gap-2 mb-1';

        var nameSpan = document.createElement('span');
        nameSpan.className = 'text-xs font-semibold ' + (role === 'user' ? 'text-indigo-400' : 'text-purple-400');
        nameSpan.textContent = role === 'user' ? 'You' : 'Helheim';
        header.appendChild(nameSpan);

        var body = document.createElement('div');
        body.className = 'text-sm leading-relaxed markdown-content';

        if (role === 'user') {
            body.textContent = content;
        } else if (isStreaming) {
            body.innerHTML = '<div class="typing-indicator"><span></span><span></span><span></span></div>';
        } else {
            body.innerHTML = this.renderMarkdown(content);
        }

        bubble.appendChild(header);
        bubble.appendChild(body);
        col.appendChild(bubble);

        // Action buttons for assistant messages
        if (role === 'assistant' && !isStreaming) {
            var actions = this.createMsgActions();
            col.appendChild(actions);
        }

        wrapper.appendChild(col);
        container.appendChild(wrapper);
        this.scrollToBottom();

        return { body: body, wrapper: wrapper, col: col, bubble: bubble };
    },

    createMsgActions: function() {
        var actions = document.createElement('div');
        actions.className = 'msg-actions flex gap-2 px-1';
        actions.innerHTML = '<button onclick="ChatActions.regenerate()" class="text-xs text-gray-600 hover:text-gray-400 cursor-pointer flex items-center gap-1">'
            + '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 4v6h6"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/></svg>'
            + 'Regenerate</button>'
            + '<button onclick="ChatActions.copyLast()" class="text-xs text-gray-600 hover:text-gray-400 cursor-pointer flex items-center gap-1">'
            + '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>'
            + 'Copy</button>';
        return actions;
    },

    addMsgActions: function(col) {
        var existing = col.querySelector('.msg-actions');
        if (!existing) {
            col.appendChild(this.createMsgActions());
        }
    },

    updateStreamContent: function(bodyEl, content) {
        bodyEl.innerHTML = this.renderMarkdown(content);
        this.scrollToBottom();
    },

    setStatus: function(text, color) {
        var el = document.getElementById('status-text');
        if (el) {
            el.textContent = text;
            el.className = 'text-xs text-' + (color || 'gray') + '-500';
        }
    },

    setMsgCount: function() {
        var el = document.getElementById('token-count');
        if (el) el.textContent = ChatState.history.length + ' messages';
    },

    showWelcome: function() {
        var container = document.getElementById('chat-messages');
        container.innerHTML = this.welcomeHTML;
    },

    toggleSettings: function() {
        var panel = document.getElementById('settings-panel');
        if (panel) panel.classList.toggle('hidden');
    },

    syncSettingsUI: function() {
        var el;
        el = document.getElementById('model-select');
        if (el) el.value = ChatState.settings.model;
        el = document.getElementById('max-tokens');
        if (el) el.value = ChatState.settings.maxTokens;
        el = document.getElementById('stream-toggle');
        if (el) el.checked = ChatState.settings.stream;
        el = document.getElementById('system-prompt');
        if (el) el.value = ChatState.systemPrompt;
        el = document.getElementById('temperature');
        if (el) el.value = ChatState.settings.temperature;
        el = document.getElementById('temp-display');
        if (el) el.textContent = ChatState.settings.temperature;
    },

    setSendButton: function(generating) {
        var btn = document.getElementById('send-btn');
        var stopBtn = document.getElementById('stop-btn');
        if (btn) btn.classList.toggle('hidden', generating);
        if (stopBtn) stopBtn.classList.toggle('hidden', !generating);
    }
};
