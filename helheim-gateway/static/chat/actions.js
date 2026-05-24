/* === Helheim Chat — Actions Module === */

var ChatActions = {
    lastBodyEl: null,
    lastCol: null,

    setPrompt: function(text) {
        var input = document.getElementById('chat-input');
        if (input) {
            input.value = text;
            input.focus();
            this.autoResize(input);
        }
    },

    handleKey: function(e) {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            ChatActions.send();
        }
    },

    autoResize: function(el) {
        el.style.height = 'auto';
        el.style.height = Math.min(el.scrollHeight, 200) + 'px';
    },

    readSettings: function() {
        var el;
        el = document.getElementById('model-select');
        if (el) ChatState.settings.model = el.value;
        el = document.getElementById('max-tokens');
        if (el) ChatState.settings.maxTokens = parseInt(el.value) || 512;
        el = document.getElementById('stream-toggle');
        if (el) ChatState.settings.stream = el.checked;
        el = document.getElementById('system-prompt');
        if (el) ChatState.systemPrompt = el.value;
        el = document.getElementById('temperature');
        if (el) ChatState.settings.temperature = parseFloat(el.value) || 0.7;
        ChatState.saveSettings();
    },

    send: function() {
        if (ChatState.isGenerating) return;
        var input = document.getElementById('chat-input');
        var text = input.value.trim();
        if (!text) return;

        input.value = '';
        this.autoResize(input);
        this.readSettings();

        // Add user message
        ChatState.addMessage('user', text);
        ChatUI.addMessage('user', text);

        // Add assistant placeholder
        var result = ChatUI.addMessage('assistant', '', true);
        this.lastBodyEl = result.body;
        this.lastCol = result.col;

        ChatState.isGenerating = true;
        ChatUI.setSendButton(true);
        ChatUI.setStatus('Generating...', 'indigo');

        var payload = {
            model: ChatState.settings.model,
            messages: ChatState.getMessages(),
            max_tokens: ChatState.settings.maxTokens,
            stream: ChatState.settings.stream
        };

        var startTime = Date.now();

        if (ChatState.settings.stream) {
            this._streamSend(payload, startTime);
        } else {
            this._fetchSend(payload, startTime);
        }
    },

    _streamSend: function(payload, startTime) {
        var self = this;
        var fullContent = '';

        ChatAPI.streamRequest(payload,
            function onChunk(text) {
                fullContent += text;
                ChatUI.updateStreamContent(self.lastBodyEl, fullContent);
            },
            function onDone() {
                self._finish(fullContent, startTime);
            },
            function onError(msg) {
                self.lastBodyEl.innerHTML = '<span class="text-red-400">Error: ' + ChatUI.escapeHtml(msg) + '</span>';
                self._finish('', startTime);
            }
        );
    },

    _fetchSend: function(payload, startTime) {
        var self = this;

        ChatAPI.fetchRequest(payload,
            function onSuccess(content) {
                ChatUI.updateStreamContent(self.lastBodyEl, content);
                self._finish(content, startTime);
            },
            function onError(msg) {
                self.lastBodyEl.innerHTML = '<span class="text-red-400">Error: ' + ChatUI.escapeHtml(msg) + '</span>';
                self._finish('', startTime);
            }
        );
    },

    _finish: function(content, startTime) {
        if (content) {
            ChatState.addMessage('assistant', content);
            ChatUI.addMsgActions(this.lastCol);
        }
        ChatState.isGenerating = false;
        ChatUI.setSendButton(false);
        var elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
        ChatUI.setStatus('Done (' + elapsed + 's)', 'green');
        ChatUI.setMsgCount();
        ChatState.loadCredits();
        document.getElementById('chat-input').focus();
    },

    stop: function() {
        ChatAPI.abort();
        ChatState.isGenerating = false;
        ChatUI.setSendButton(false);
        ChatUI.setStatus('Stopped', 'yellow');
    },

    regenerate: function() {
        if (ChatState.isGenerating) return;
        if (!ChatState.removeLastAssistant()) return;

        // Remove last assistant bubble from DOM
        var container = document.getElementById('chat-messages');
        var wrappers = container.querySelectorAll('.msg-wrapper');
        for (var i = wrappers.length - 1; i >= 0; i--) {
            var name = wrappers[i].querySelector('.text-purple-400');
            if (name) {
                wrappers[i].remove();
                break;
            }
        }

        // Re-send with same history
        this.readSettings();
        var result = ChatUI.addMessage('assistant', '', true);
        this.lastBodyEl = result.body;
        this.lastCol = result.col;

        ChatState.isGenerating = true;
        ChatUI.setSendButton(true);
        ChatUI.setStatus('Regenerating...', 'indigo');

        var payload = {
            model: ChatState.settings.model,
            messages: ChatState.getMessages(),
            max_tokens: ChatState.settings.maxTokens,
            stream: ChatState.settings.stream
        };

        var startTime = Date.now();
        if (ChatState.settings.stream) {
            this._streamSend(payload, startTime);
        } else {
            this._fetchSend(payload, startTime);
        }
    },

    copyLast: function() {
        var last = ChatState.history[ChatState.history.length - 1];
        if (last && last.role === 'assistant') {
            navigator.clipboard.writeText(last.content).then(function() {
                ChatUI.setStatus('Copied!', 'green');
                setTimeout(function() { ChatUI.setStatus('Ready', 'gray'); }, 1500);
            });
        }
    },

    clear: function() {
        ChatState.clear();
        ChatUI.showWelcome();
        ChatUI.setStatus('Ready', 'gray');
        ChatUI.setMsgCount();
    },

    exportTxt: function() {
        var text = ChatState.exportChat();
        this._download('helheim-chat.txt', text, 'text/plain');
    },

    exportJson: function() {
        var json = ChatState.exportJSON();
        this._download('helheim-chat.json', json, 'application/json');
    },

    _download: function(filename, content, mime) {
        var blob = new Blob([content], { type: mime });
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url;
        a.download = filename;
        a.click();
        URL.revokeObjectURL(url);
    }
};
