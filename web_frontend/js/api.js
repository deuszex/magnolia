// API fetch wrapper with session cookie
var api = (function () {
    async function request(method, path, body) {
        var opts = {
            method: method,
            credentials: 'include',
            headers: {}
        };
        if (body !== undefined) {
            opts.headers['Content-Type'] = 'application/json';
            opts.body = JSON.stringify(body);
        }
        var res = await fetch(path, opts);
        if (res.status === 204) return null;
        var text = await res.text();
        var data = text ? JSON.parse(text) : null;
        if (!res.ok) {
            var msg = (data && data.error) || res.statusText || 'Request failed';
            throw new Error(msg);
        }
        return data;
    }

    // Upload a file via multipart/form-data. Returns parsed JSON response.
    // fields: object of key-value pairs; file, filename, fieldName come from params.
    async function upload(path, file, extraFields) {
        var form = new FormData();
        form.append('file', file, file.name);
        if (extraFields) {
            Object.keys(extraFields).forEach(function (k) {
                form.append(k, extraFields[k]);
            });
        }
        var res = await fetch(path, {
            method: 'POST',
            credentials: 'include',
            body: form
        });
        var text = await res.text();
        var data = text ? JSON.parse(text) : null;
        if (!res.ok) {
            var msg = (data && data.error) || res.statusText || 'Upload failed';
            throw new Error(msg);
        }
        return data;
    }

    return {
        get: function (path) { return request('GET', path); },
        post: function (path, body) { return request('POST', path, body); },
        put: function (path, body) { return request('PUT', path, body); },
        patch: function (path, body) { return request('PATCH', path, body); },
        del: function (path) { return request('DELETE', path); },
        upload: upload,

        isAuthenticated: async function () {
            try {
                var user = await request('GET', '/api/auth/me');
                return user;
            } catch (_) {
                return null;
            }
        }
    };
})();
