// End-to-end encryption for direct messages
// Uses Web Crypto API: ECDH P-256 key exchange + AES-256-GCM encryption
// Private key is wrapped with AES-256-GCM derived from a user passphrase via PBKDF2.
//
// Storage format v2 (server-side, GET/PUT /api/auth/me/e2e-key):
//   { v: 2, salt: <b64>, iv: <b64>, data: <b64 encrypted private key JWK>, pub: <JWK object> }
// The server stores the blob opaquely and cannot decrypt it.
var e2e = (function () {
    var keyPair = null; // { privateKey, publicKey } — CryptoKey objects
    var initPromise = null; // guards against concurrent init() calls
    var pubKeyCache = {}; // userId -> CryptoKey (recipient public keys)
    var sharedKeyCache = {}; // userId -> CryptoKey (derived AES keys)

    var ECDH_ALGO = { name: 'ECDH', namedCurve: 'P-256' };
    var AES_ALGO = { name: 'AES-GCM', length: 256 };
    var PBKDF2_ITERS = 600000;
    var PASSPHRASE_MIN_LEN = 12;

    // Detect encrypted message envelope
    function isEncrypted(content) {
        return typeof content === 'string' && content.startsWith('{"v":1,');
    }

    // --- Key wrapping helpers ---

    // Derive an AES-256-GCM wrapping key from passphrase + salt using PBKDF2-SHA-256
    async function deriveWrappingKey(passphrase, salt) {
        var enc = new TextEncoder();
        var base = await crypto.subtle.importKey(
            'raw', enc.encode(passphrase), 'PBKDF2', false, ['deriveKey']
        );
        return crypto.subtle.deriveKey(
            { name: 'PBKDF2', salt: salt, iterations: PBKDF2_ITERS, hash: 'SHA-256' },
            base,
            { name: 'AES-GCM', length: 256 },
            false,
            ['encrypt', 'decrypt']
        );
    }

    // Encrypt the private key JWK with passphrase — returns { salt, iv, data } (all base64)
    async function wrapPrivateKey(privateKey, passphrase) {
        var salt = crypto.getRandomValues(new Uint8Array(16));
        var iv = crypto.getRandomValues(new Uint8Array(12));
        var wrappingKey = await deriveWrappingKey(passphrase, salt);
        var privJwk = await crypto.subtle.exportKey('jwk', privateKey);
        var enc = new TextEncoder();
        var ciphertext = await crypto.subtle.encrypt(
            { name: 'AES-GCM', iv: iv },
            wrappingKey,
            enc.encode(JSON.stringify(privJwk))
        );
        return { salt: _b64(salt), iv: _b64(iv), data: _b64(new Uint8Array(ciphertext)) };
    }

    // Decrypt the private key JWK — throws DOMException on wrong passphrase
    async function unwrapPrivateKey(blob, passphrase) {
        var salt = _fromb64(blob.salt);
        var iv = _fromb64(blob.iv);
        var data = _fromb64(blob.data);
        var wrappingKey = await deriveWrappingKey(passphrase, salt);
        var plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: iv }, wrappingKey, data);
        var privJwk = JSON.parse(new TextDecoder().decode(plain));
        return crypto.subtle.importKey('jwk', privJwk, ECDH_ALGO, false, ['deriveKey']);
    }

    // --- Passphrase dialog ---

    // Shows a modal passphrase dialog and returns Promise<string>.
    // Rejects with Error('cancelled') if the user dismisses without submitting.
    // errorMsg: optional string displayed above the input (used when last attempt failed).
    function promptPassphrase(isNew, errorMsg) {
        return new Promise(function (resolve, reject) {
            var overlay = document.createElement('div');
            overlay.style.cssText = 'position:fixed;inset:0;background:rgba(0,0,0,.55);z-index:9999;display:flex;align-items:center;justify-content:center';

            var box = document.createElement('div');
            box.className = 'dialog';

            var title = document.createElement('h3');
            title.style.marginTop = '0';
            title.textContent = isNew ? 'Create E2E Encryption Passphrase' : 'Unlock E2E Encryption';

            var desc = document.createElement('p');
            desc.style.cssText = 'font-size:13px;opacity:.7;margin:0 0 16px';
            desc.textContent = isNew
                ? 'Choose a passphrase to protect your private message key. You will need this each session to decrypt messages.'
                : 'Enter your passphrase to unlock end-to-end encrypted messages.';

            var errEl = document.createElement('div');
            errEl.style.cssText = 'color:#c00;font-size:12px;margin-bottom:10px';
            errEl.textContent = errorMsg || '';
            errEl.style.display = errorMsg ? '' : 'none';

            var passLabel = document.createElement('label');
            passLabel.textContent = 'Passphrase';
            passLabel.style.cssText = 'display:block;font-size:13px;font-weight:600;margin-bottom:4px';

            var passInput = document.createElement('input');
            passInput.type = 'password';
            passInput.className = 'form-input';
            passInput.style.cssText = 'width:100%;box-sizing:border-box;margin-bottom:12px';
            passInput.autocomplete = isNew ? 'new-password' : 'current-password';
            passInput.placeholder = isNew ? 'At least ' + PASSPHRASE_MIN_LEN + ' characters' : '';

            var confirmLabel, confirmInput;
            if (isNew) {
                confirmLabel = document.createElement('label');
                confirmLabel.textContent = 'Confirm Passphrase';
                confirmLabel.style.cssText = 'display:block;font-size:13px;font-weight:600;margin-bottom:4px';

                confirmInput = document.createElement('input');
                confirmInput.type = 'password';
                confirmInput.className = 'form-input';
                confirmInput.style.cssText = 'width:100%;box-sizing:border-box;margin-bottom:12px';
                confirmInput.autocomplete = 'new-password';
                confirmInput.placeholder = 'Repeat passphrase';
            }

            var inlineErr = document.createElement('div');
            inlineErr.style.cssText = 'color:#c00;font-size:12px;margin-bottom:10px;display:none';

            var actions = document.createElement('div');
            actions.className = 'dialog-actions';

            var skipBtn = document.createElement('button');
            skipBtn.textContent = 'Skip';
            skipBtn.className = 'btn btn-secondary';
            skipBtn.title = 'Messages will not be encrypted this session';
            skipBtn.type = 'button';

            var submitBtn = document.createElement('button');
            submitBtn.textContent = isNew ? 'Create' : 'Unlock';
            submitBtn.className = 'btn btn-primary';
            submitBtn.type = 'button';

            function close() { document.body.removeChild(overlay); }

            function showInlineErr(msg) {
                inlineErr.textContent = msg;
                inlineErr.style.display = '';
                passInput.focus();
            }

            skipBtn.onclick = function () {
                close();
                reject(new Error('cancelled'));
            };

            submitBtn.onclick = function () {
                var pass = passInput.value;
                if (!pass) { showInlineErr('Passphrase cannot be empty.'); return; }
                if (isNew && pass.length < PASSPHRASE_MIN_LEN) {
                    showInlineErr('Passphrase must be at least ' + PASSPHRASE_MIN_LEN + ' characters.');
                    return;
                }
                if (isNew && confirmInput.value !== pass) {
                    showInlineErr('Passphrases do not match.');
                    confirmInput.focus();
                    return;
                }
                close();
                resolve(pass);
            };

            var lastInput = confirmInput || passInput;
            lastInput.addEventListener('keydown', function (e) {
                if (e.key === 'Enter') submitBtn.click();
            });
            if (confirmInput) {
                passInput.addEventListener('keydown', function (e) {
                    if (e.key === 'Enter') confirmInput.focus();
                });
            }

            actions.appendChild(skipBtn);
            actions.appendChild(submitBtn);
            box.appendChild(title);
            box.appendChild(desc);
            if (errorMsg) box.appendChild(errEl);
            box.appendChild(passLabel);
            box.appendChild(passInput);
            if (isNew) {
                box.appendChild(confirmLabel);
                box.appendChild(confirmInput);
            }
            box.appendChild(inlineErr);
            box.appendChild(actions);
            overlay.appendChild(box);
            document.body.appendChild(overlay);

            setTimeout(function () { passInput.focus(); }, 50);
        });
    }

    // --- Init ---

    function init() {
        if (!initPromise) initPromise = _init();
        return initPromise;
    }

    async function _init() {
        if (!window.crypto || !window.crypto.subtle) {
            console.warn('E2E: Web Crypto API not available — messages will be plaintext');
            return;
        }

        // Fetch the encrypted blob from the server (null if this is the user's first device)
        var blob = null;
        try {
            var resp = await api.get('/api/auth/me/e2e-key');
            if (resp && resp.e2e_key_blob) {
                blob = JSON.parse(resp.e2e_key_blob);
            }
        } catch (e) {
            console.warn('E2E: Failed to fetch key blob from server:', e);
        }

        if (blob) {
            if (blob.v !== 2 || !blob.salt || !blob.iv || !blob.data || !blob.pub) {
                // Unrecognised or corrupt format — treat as no key and generate a fresh one
                blob = null;
            } else {
                // Existing key — prompt for passphrase to unwrap, retry on wrong passphrase
                var errorMsg = null;
                while (!keyPair) {
                    var passphrase;
                    try {
                        passphrase = await promptPassphrase(false, errorMsg);
                    } catch (_) {
                        // User skipped — leave keyPair null, graceful degradation applies
                        return;
                    }
                    try {
                        var privateKey = await unwrapPrivateKey(blob, passphrase);
                        var publicKey = await crypto.subtle.importKey('jwk', blob.pub, ECDH_ALGO, true, []);
                        keyPair = { privateKey: privateKey, publicKey: publicKey };
                    } catch (_) {
                        errorMsg = 'Wrong passphrase — please try again.';
                    }
                }
            }
        }

        if (!keyPair) {
            // No key on server (first device) — generate a new key pair and store it
            var newPair = await crypto.subtle.generateKey(ECDH_ALGO, true, ['deriveKey']);
            var passphrase;
            try {
                passphrase = await promptPassphrase(true, null);
            } catch (_) {
                // User skipped setup — no E2E this session, nothing to store
                return;
            }
            var wrapped = await wrapPrivateKey(newPair.privateKey, passphrase);
            var pubJwk = await crypto.subtle.exportKey('jwk', newPair.publicKey);
            var blobJson = JSON.stringify({
                v: 2,
                salt: wrapped.salt,
                iv: wrapped.iv,
                data: wrapped.data,
                pub: pubJwk
            });
            try {
                await api.put('/api/auth/me/e2e-key', { e2e_key_blob: blobJson });
            } catch (e) {
                console.warn('E2E: Failed to store key blob on server:', e);
                // Proceed with the in-memory key for this session
            }
            keyPair = newPair;
        }

        // Upload public key so others can encrypt messages to us
        try {
            var pubJwk = await crypto.subtle.exportKey('jwk', keyPair.publicKey);
            await api.put('/api/auth/me/public-key', { public_key: JSON.stringify(pubJwk) });
        } catch (e) {
            console.warn('E2E: Failed to upload public key:', e);
        }
    }

    // --- Core encryption API (unchanged) ---

    // Fetch and cache a recipient's ECDH public key from the server
    async function getRecipientKey(userId) {
        if (pubKeyCache[userId]) return pubKeyCache[userId];
        try {
            var data = await api.get('/api/users/' + userId + '/profile');
            if (!data.public_key) return null;
            var jwk = JSON.parse(data.public_key);
            var key = await crypto.subtle.importKey('jwk', jwk, ECDH_ALGO, false, []);
            pubKeyCache[userId] = key;
            return key;
        } catch (e) {
            console.warn('E2E: Failed to fetch public key for user', e);
            return null;
        }
    }

    // Derive or retrieve cached AES-256-GCM key via ECDH with the given party
    async function getSharedKey(userId) {
        if (sharedKeyCache[userId]) return sharedKeyCache[userId];
        if (!keyPair) return null;
        var theirKey = await getRecipientKey(userId);
        if (!theirKey) return null;
        var aesKey = await crypto.subtle.deriveKey(
            { name: 'ECDH', public: theirKey },
            keyPair.privateKey,
            AES_ALGO,
            false,
            ['encrypt', 'decrypt']
        );
        sharedKeyCache[userId] = aesKey;
        return aesKey;
    }

    // Encrypt plaintext for a recipient — returns JSON envelope string.
    // Throws if encryption is not possible (no local key or recipient has no key).
    async function encrypt(plaintext, recipientUserId) {
        if (!plaintext) return plaintext;
        if (!keyPair) throw new Error('E2E keys not initialised — cannot send encrypted message');
        var aesKey = await getSharedKey(recipientUserId);
        if (!aesKey) throw new Error('Recipient has no encryption key on record — cannot send encrypted message');
        var iv = crypto.getRandomValues(new Uint8Array(12));
        var enc = new TextEncoder();
        var ciphertext = await crypto.subtle.encrypt(
            { name: 'AES-GCM', iv: iv },
            aesKey,
            enc.encode(plaintext)
        );
        return JSON.stringify({
            v: 1,
            iv: _b64(iv),
            data: _b64(new Uint8Array(ciphertext)),
        });
    }

    // Decrypt an envelope — returns plaintext, '[encrypted]', or the original string
    async function decrypt(content, senderUserId) {
        if (!isEncrypted(content)) return content; // legacy plaintext
        if (!keyPair) return '[encrypted]';
        try {
            var envelope = JSON.parse(content);
            var aesKey = await getSharedKey(senderUserId);
            if (!aesKey) return '[encrypted — key unavailable]';
            var iv = _fromb64(envelope.iv);
            var data = _fromb64(envelope.data);
            var plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: iv }, aesKey, data);
            return new TextDecoder().decode(plain);
        } catch (e) {
            return '[decryption failed]';
        }
    }

    // Evict cached shared key for a user (call when conversation is closed)
    function evictCache(userId) {
        delete sharedKeyCache[userId];
        delete pubKeyCache[userId];
    }

    function _b64(arr) {
        return btoa(String.fromCharCode.apply(null, arr));
    }

    function _fromb64(b64) {
        return Uint8Array.from(atob(b64), function (c) { return c.charCodeAt(0); });
    }

    return { init: init, encrypt: encrypt, decrypt: decrypt, isEncrypted: isEncrypted, evictCache: evictCache };
})();
