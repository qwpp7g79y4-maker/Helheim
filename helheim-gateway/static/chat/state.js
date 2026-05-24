/* === Helheim Chat — State Management === */

var ChatState = {
    apiKey: '',
    history: [],
    isGenerating: false,
    abortController: null,
    systemPrompt: '',
    settings: {
        model: 'auto',
        maxTokens: 512,
        stream: true,
        temperature: 0.7
    },

    init: function(apiKey) {
        this.apiKey = apiKey;
        this.loadSettings();
        this.loadModels();
        this.loadCredits();
    },

    loadCredits: function() {
        var self = this;
        fetch('/api/v1/credits', { headers: { 'Authorization': 'Bearer ' + this.apiKey } })
            .then(function(r) { return r.json(); })
            .then(function(d) {
                var credits = (d && d.credits !== undefined) ? d.credits : 0;
                var el = document.getElementById('nav-credits');
                if (el) { el.textContent = credits.toLocaleString() + ' credits'; }
                var banner = document.getElementById('low-credits-banner');
                if (banner) {
                    if (credits <= 100) banner.classList.remove('hidden');
                    else banner.classList.add('hidden');
                }
            })
            .catch(function() {});
    },

    loadSettings: function() {
        try {
            var saved = localStorage.getItem('helheim_chat_settings');
            if (saved) {
                var parsed = JSON.parse(saved);
                Object.assign(this.settings, parsed);
            }
            var sp = localStorage.getItem('helheim_system_prompt');
            if (sp) this.systemPrompt = sp;
        } catch(e) {}
    },

    saveSettings: function() {
        try {
            localStorage.setItem('helheim_chat_settings', JSON.stringify(this.settings));
            localStorage.setItem('helheim_system_prompt', this.systemPrompt);
        } catch(e) {}
    },

    loadModels: function() {
        fetch('/api/v1/models').then(function(r) { return r.json(); }).then(function(models) {
            var sel = document.getElementById('model-select');
            if (!sel) return;
            sel.innerHTML = '';
            var autoOpt = document.createElement('option');
            autoOpt.value = 'auto';
            autoOpt.textContent = 'Auto (best available)';
            sel.appendChild(autoOpt);

            var groups = {};
            models.forEach(function(m) {
                var cat = m.category || 'general';
                if (!groups[cat]) groups[cat] = [];
                groups[cat].push(m);
            });

            Object.keys(groups).forEach(function(cat) {
                var group = document.createElement('optgroup');
                group.label = cat.charAt(0).toUpperCase() + cat.slice(1);
                groups[cat].forEach(function(m) {
                    var opt = document.createElement('option');
                    opt.value = m.id;
                    var tag = m.tag ? ' [' + m.tag + ']' : '';
                    opt.textContent = m.name + ' (' + m.params + ')' + tag;
                    group.appendChild(opt);
                });
                sel.appendChild(group);
            });

            sel.value = ChatState.settings.model;
        }).catch(function() {});
    },

    addMessage: function(role, content) {
        this.history.push({ role: role, content: content });
    },

    getMessages: function() {
        var msgs = [];
        if (this.systemPrompt.trim()) {
            msgs.push({ role: 'system', content: this.systemPrompt.trim() });
        }
        return msgs.concat(this.history);
    },

    removeLastAssistant: function() {
        for (var i = this.history.length - 1; i >= 0; i--) {
            if (this.history[i].role === 'assistant') {
                this.history.splice(i, 1);
                return true;
            }
        }
        return false;
    },

    clear: function() {
        this.history = [];
    },

    exportChat: function() {
        var text = '';
        if (this.systemPrompt) text += '[System] ' + this.systemPrompt + '\n\n';
        this.history.forEach(function(m) {
            var role = m.role === 'user' ? 'You' : 'Helheim';
            text += '[' + role + ']\n' + m.content + '\n\n';
        });
        return text;
    },

    exportJSON: function() {
        return JSON.stringify({
            system_prompt: this.systemPrompt,
            model: this.settings.model,
            messages: this.history,
            exported_at: new Date().toISOString()
        }, null, 2);
    }
};
