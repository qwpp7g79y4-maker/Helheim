/* === Helheim Chat — API Module === */

var ChatAPI = {
    baseUrl: '',

    streamRequest: function(payload, onChunk, onDone, onError) {
        ChatState.abortController = new AbortController();

        fetch(this.baseUrl + '/v1/chat/completions', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': 'Bearer ' + ChatState.apiKey
            },
            body: JSON.stringify(payload),
            signal: ChatState.abortController.signal
        }).then(function(response) {
            if (!response.ok) throw new Error('HTTP ' + response.status);
            var reader = response.body.getReader();
            var decoder = new TextDecoder();
            var buffer = '';

            function read() {
                reader.read().then(function(result) {
                    if (result.done) { onDone(); return; }

                    buffer += decoder.decode(result.value, { stream: true });
                    var lines = buffer.split('\n');
                    buffer = lines.pop() || '';

                    for (var i = 0; i < lines.length; i++) {
                        var line = lines[i].trim();
                        if (!line.startsWith('data: ')) continue;
                        var data = line.substring(6);
                        if (data === '[DONE]') { onDone(); return; }
                        try {
                            var chunk = JSON.parse(data);
                            if (chunk.error && chunk.error.message) {
                                onError(chunk.error.message);
                                return;
                            }
                            var delta = chunk.choices && chunk.choices[0] && chunk.choices[0].delta;
                            if (delta && delta.content) {
                                onChunk(delta.content);
                            }
                        } catch (e) {}
                    }
                    read();
                }).catch(function(err) {
                    if (err.name === 'AbortError') {
                        onDone();
                    } else {
                        onError(err.message);
                    }
                });
            }
            read();
        }).catch(function(err) {
            if (err.name === 'AbortError') {
                onDone();
            } else {
                onError(err.message);
            }
        });
    },

    fetchRequest: function(payload, onSuccess, onError) {
        ChatState.abortController = new AbortController();

        fetch(this.baseUrl + '/v1/chat/completions', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': 'Bearer ' + ChatState.apiKey
            },
            body: JSON.stringify(payload),
            signal: ChatState.abortController.signal
        }).then(function(r) { return r.json(); })
        .then(function(data) {
            if (data.error) {
                onError(data.error.message || JSON.stringify(data.error));
                return;
            }
            var content = data.choices[0].message.content;
            var usage = data.usage || {};
            onSuccess(content, usage);
        }).catch(function(err) {
            if (err.name !== 'AbortError') {
                onError(err.message);
            }
        });
    },

    abort: function() {
        if (ChatState.abortController) {
            ChatState.abortController.abort();
            ChatState.abortController = null;
        }
    }
};
