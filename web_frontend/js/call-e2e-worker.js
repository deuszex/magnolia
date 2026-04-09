/**
 * WebRTC E2E encryption worker
 *
 * Runs inside a dedicated Worker. Generates an ephemeral ECDH P-256 key pair
 * per call, derives a shared AES-256-GCM key via ECDH with each peer, then
 * encrypts/decrypts each encoded media frame via RTCRtpScriptTransform.
 *
 * Supports group calls: each peer pair has its own independent AES-256-GCM key,
 * stored in a per-peer map keyed by peerId.
 *
 * Frame wire format (matching native GUI encryption.rs):
 * [8-byte counter LE] [ciphertext] [16-byte AES-GCM tag]
 * Nonce (12 bytes): [4 zero bytes] [8-byte counter LE]
 */
'use strict';

// Startup probe — confirms the worker script loaded and executed
self.postMessage({ type: 'log', msg: 'worker started — typeof RTCRtpScriptTransform: ' + typeof RTCRtpScriptTransform + ' | typeof self.onrtctransform: ' + typeof self.onrtctransform });

var ownPrivateKey = null; // ECDH CryptoKey (non-extractable)
var sharedKeys = {}; // { peerId: CryptoKey } — one AES-256-GCM key per peer
var sendCounters = {}; // { peerId: number } — monotonic per-peer frame counters

// Diagnostic counters — reset each call
var _firstFrame = {}; // { "op:peerId": true } once logged

// Main-thread communication 

self.onmessage = async function (event) {
    var msg = event.data;

    if (msg.type === 'init') {
        try {
            var kp = await crypto.subtle.generateKey(
                { name: 'ECDH', namedCurve: 'P-256' },
                true,
                ['deriveKey']
            );
            ownPrivateKey = kp.privateKey;
            var pubRaw = await crypto.subtle.exportKey('raw', kp.publicKey);
            self.postMessage({ type: 'public_key', bytes: new Uint8Array(pubRaw) });
        } catch (e) {
            self.postMessage({ type: 'error', reason: 'keygen_failed: ' + e.message });
        }
        return;
    }

    if (msg.type === 'peer_key') {
        var peerId = msg.peerId; // userId of the peer
        var peerBytes = msg.bytes; // Uint8Array — peer's raw P-256 public key (65 bytes uncompressed)
        if (!ownPrivateKey) {
            self.postMessage({ type: 'error', reason: 'own_key_not_ready', peerId: peerId });
            return;
        }
        if (peerBytes.length !== 65) {
            // Not a P-256 uncompressed key — incompatible peer (e.g. native GUI using X25519)
            self.postMessage({ type: 'key_error', reason: 'incompatible_curve', peerId: peerId });
            return;
        }
        try {
            var peerKey = await crypto.subtle.importKey(
                'raw', peerBytes,
                { name: 'ECDH', namedCurve: 'P-256' },
                false, []
            );
            sharedKeys[peerId] = await crypto.subtle.deriveKey(
                { name: 'ECDH', public: peerKey },
                ownPrivateKey,
                { name: 'AES-GCM', length: 256 },
                false,
                ['encrypt', 'decrypt']
            );
            sendCounters[peerId] = 0;
            self.postMessage({ type: 'key_ready', peerId: peerId });
        } catch (e) {
            self.postMessage({ type: 'key_error', reason: e.message, peerId: peerId });
        }
        return;
    }

    if (msg.type === 'remove_peer') {
        delete sharedKeys[msg.peerId];
        delete sendCounters[msg.peerId];
        return;
    }
};

// RTCRtpScriptTransform handler 

function handleRtcTransform(event) {
    var transformer = event.transformer;
    // postMessage first — before any property access that could throw
    self.postMessage({ type: 'log', msg: 'onrtctransform FIRED — event.type: ' + event.type + ' transformer: ' + (transformer ? 'present' : 'MISSING') });

    var operation = transformer.options.operation; // 'encrypt' or 'decrypt'
    var peerId = transformer.options.peerId; // which peer this transform is for
    var trackKind = transformer.options.trackKind || 'unknown'; // 'audio' or 'video'

    self.postMessage({ type: 'log', msg: 'onrtctransform ' + operation + ' [' + trackKind + '] for ' + peerId + ' — readable: ' + (transformer.readable ? 'present' : 'MISSING') + ' writable: ' + (transformer.writable ? 'present' : 'MISSING') });

    transformer.readable
        .pipeThrough(new TransformStream({
            transform: function (frame, controller) {
                var key = sharedKeys[peerId];
                var tag = operation + ':' + peerId + ':' + trackKind;
                var count = (_firstFrame[tag] || 0) + 1;
                _firstFrame[tag] = count;
                // Log every frame for the first 5, then every 100th
                if (count <= 5 || count % 100 === 0) {
                    self.postMessage({
                        type: 'log',
                        msg: operation + ' [' + trackKind + '] frame #' + count + ' for ' + peerId +
                            ' bytes:' + frame.data.byteLength +
                            ' key:' + (key ? 'ready' : 'NOT READY')
                    });
                }
                if (!key) {
                    // Key exchange not yet complete — pass frame through unmodified.
                    // Still protected by DTLS-SRTP; E2E kicks in once the key is ready.
                    controller.enqueue(frame);
                    return;
                }
                if (operation === 'encrypt') {
                    return encryptFrame(frame, controller, peerId, key);
                } else {
                    return decryptFrame(frame, controller, key, peerId);
                }
            },
            flush: function () {
                self.postMessage({ type: 'log', msg: operation + ' pipeline flushed for ' + peerId });
            }
        }))
        .pipeTo(transformer.writable)
        .catch(function (e) {
            self.postMessage({ type: 'log', msg: operation + ' pipeline error for ' + peerId + ': ' + (e && e.message ? e.message : String(e)) });
        });
}

// Register via both mechanisms to handle browser differences:
// - Desktop Chrome dispatches via onrtctransform
// - Some mobile Chrome versions only dispatch via addEventListener
// Guard against double-processing using readable.locked — if pipeThrough has
// already been called for this transformer, the readable will be skip and
// we lock.

function handleRtcTransformOnce(event) {
    if (event.transformer.readable.locked) {
        self.postMessage({ type: 'log', msg: 'onrtctransform skipped (readable already locked) for ' + (event.transformer.options && event.transformer.options.operation) });
        return;
    }
    handleRtcTransform(event);
}

self.onrtctransform = handleRtcTransformOnce;
self.addEventListener('rtctransform', handleRtcTransformOnce);

self.postMessage({
    type: 'transform_registered',
    methods: ['onrtctransform', 'addEventListener(rtctransform)'],
    hasCryptoSubtle: typeof crypto !== 'undefined' && typeof crypto.subtle !== 'undefined',
});

// Frame encrypt / decrypt 

function encryptFrame(frame, controller, peerId, key) {
    var counter = sendCounters[peerId]++;
    var nonce = buildNonce(counter);
    var plain = frame.data;

    return crypto.subtle.encrypt({ name: 'AES-GCM', iv: nonce }, key, plain)
        .then(function (cipher) {
            var out = new ArrayBuffer(8 + cipher.byteLength);
            var dv = new DataView(out);
            // Write 64-bit counter as two 32-bit LE words
            dv.setUint32(0, counter >>> 0, true);
            dv.setUint32(4, Math.floor(counter / 0x100000000) >>> 0, true);
            new Uint8Array(out, 8).set(new Uint8Array(cipher));
            frame.data = out;
            controller.enqueue(frame);
        })
        .catch(function () { /* drop frame on encryption error */ });
}

var _decryptOk = {};
var _decryptFail = {};

function decryptFrame(frame, controller, key, peerId) {
    // Minimum: 8-byte counter + 16-byte GCC tag = 24 bytes
    if (frame.data.byteLength < 24) {
        // Frame too short to be encrypted — pass through (unkeyed warm-up frames)
        controller.enqueue(frame);
        return;
    }

    var dv = new DataView(frame.data);
    var ctrLow = dv.getUint32(0, true);
    var ctrHigh = dv.getUint32(4, true);
    var counter = ctrLow + ctrHigh * 0x100000000;
    var nonce = buildNonce(counter);
    var cipher = frame.data.slice(8); // ciphertext + GCM tag

    return crypto.subtle.decrypt({ name: 'AES-GCM', iv: nonce }, key, cipher)
        .then(function (plain) {
            _decryptOk[peerId] = (_decryptOk[peerId] || 0) + 1;
            var okCount = _decryptOk[peerId];
            frame.data = plain;
            // Verify the assignment actually updated the backing buffer
            var afterBytes = frame.data ? frame.data.byteLength : -1;
            if (okCount <= 3) {
                self.postMessage({ type: 'log', msg: 'decrypt enqueue #' + okCount + ' for ' + peerId +
                    ' plain:' + plain.byteLength + ' frame.data.byteLength after assign:' + afterBytes +
                    ' match:' + (afterBytes === plain.byteLength) });
            }
            controller.enqueue(frame);
        })
        .catch(function (err) {
            _decryptFail[peerId] = (_decryptFail[peerId] || 0) + 1;
            if (_decryptFail[peerId] <= 5) {
                self.postMessage({ type: 'log', msg: 'decrypt FAILED #' + _decryptFail[peerId] + ' for ' + peerId +
                    ' cipher:' + cipher.byteLength + ' err:' + (err && err.message ? err.message : String(err)) });
            }
        });
}

function buildNonce(counter) {
    // 12-byte nonce: [4 zero bytes] [8-byte counter LE]
    var nonce = new Uint8Array(12);
    var dv = new DataView(nonce.buffer);
    dv.setUint32(4, counter >>> 0, true);
    dv.setUint32(8, Math.floor(counter / 0x100000000) >>> 0, true);
    return nonce;
}
