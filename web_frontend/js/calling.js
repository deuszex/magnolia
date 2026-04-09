/**
 * Calling module — WebSocket signaling + WebRTC peer connections.
 * Supports 1-on-1 and group (mesh) voice/video calls.
 *
 * Dual-mode:
 * Main tab — WS for signaling, shows incoming call banner, opens call tab
 * Call tab — opened via #call/out/{convId}/{type} or #call/in/{callId}/{type}/{convId}
 * connects its own WS, handles all WebRTC, full-screen call UI
 * 
 * Personal place of way too much suffering. Might redo later? Leaving console.logs in case (commented out).
 */
var calling = (function () {
    'use strict';

    // Mode Detection 
    // Call tab routes:
    // #call/out/{convId}/{type} — outgoing call
    // #call/in/{callId}/{type}/{convId} — incoming call (accept)
    // #call/join/{callId}/{type}/{convId} — join ongoing call
    function parseCallHash() {
        var hash = window.location.hash.replace('#', '');
        if (hash.indexOf('call/') !== 0) return null;
        var parts = hash.split('/');
        if (parts[1] === 'out' && parts.length >= 4) {
            return { mode: 'out', conversationId: parts[2], callType: parts[3] };
        }
        if (parts[1] === 'in' && parts.length >= 5) {
            return { mode: 'in', callId: parts[2], callType: parts[3], conversationId: parts[4] };
        }
        if (parts[1] === 'join' && parts.length >= 5) {
            return { mode: 'join', callId: parts[2], callType: parts[3], conversationId: parts[4] };
        }
        return null;
    }

    var isCallTab = !!parseCallHash();

    // State 
    var ws = null;
    var wsReconnectTimer = null;
    var wsIntentionalClose = false;

    var peerConnections = {}; // { userId: RTCPeerConnection }
    var remoteStreams = {}; // { userId: MediaStream }
    var pendingIceCandidates = {}; // { userId: RTCIceCandidateInit[] } — buffered before remoteDescription is set
    var localStream = null;

    var currentCallId = null;
    var currentCallType = null; // 'voice' or 'video'
    var currentConversationId = null;
    var callState = 'idle'; // idle, outgoing, incoming, connecting, connected
    var iceConfig = null;

    var callStartTime = null;
    var callTimerInterval = null;

    var incomingCallData = null; // stash for incoming call before accept/reject

    var isCallInitiator = false; // true if we originated the current call
    var hasJoinedCall = false; // true once we've sent call_accept/call_join/call_initiate
    var participantNames = {}; // { userId: displayName }
    var remoteVolumes = {}; // { userId: 0.0-1.0 } per-peer volume

    // Device Selection State 
    var selectedAudioInput = null;
    var selectedAudioOutput = null;
    var selectedVideoInput = null;

    // Screen Share State 
    var isScreenSharing = false;
    var screenStream = null;
    var blackVideoTrack = null; // Placeholder sent when we have no camera (keeps SSRC negotiated)

    // E2E Frame Encryption State 
    // Uses RTCRtpScriptTransform + ECDH P-256 + AES-256-GCM.
    // Only enabled when RTCRtpScriptTransform is supported by the browser.
    var callWorker = null; // Worker running call-e2e-worker.js
    var callWorkerPublicKey = null; // Uint8Array: our ephemeral P-256 public key (65 bytes)
    var pendingKeyTargets = []; // User IDs to send key_exchange to once our key is ready
    var keySentTo = {}; // { userId: true } — avoid double-sending key_exchange

    // RTCRtpScriptTransform-based E2E frame encryption.
    // Re-enabled for diagnostics — logging added to both calling.js and call-e2e-worker.js
    // to find out why onrtctransform was not firing previously.
    var E2E_SUPPORTED = typeof RTCRtpScriptTransform !== 'undefined';
    /*console.log('[E2E] RTCRtpScriptTransform available:', E2E_SUPPORTED,
        '| typeof:', typeof RTCRtpScriptTransform);*/

    // Sender (Microphone) Quality State 
    var voiceQuality = {
        preset: 'medium', // 'low', 'medium', 'high'
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true
    };

    // Preset definitions: maxBitrate (bps) for send quality
    var PRESETS = {
        low: { maxBitrate: 12000 },
        medium: { maxBitrate: 32000 },
        high: { maxBitrate: 64000 }
    };

    // Receiver (Playback) State 
    var receiverVolume = 1.0; // 0.0 – 1.0

    // DOM references (set in init) 
    var callOverlay, callStatusText, callTimerEl, remoteStreamsEl, localVideoEl;
    var btnToggleMute, btnToggleCamera, btnEndCall, btnVoiceQuality, btnScreenShare;
    var voiceQualityPanel;
    var incomingCallBanner, incomingCallerName, incomingCallType, btnAcceptCall, btnRejectCall;

    function checkMediaSupport() {
        if (!window.isSecureContext) {
            return 'Calls require a secure connection (HTTPS or localhost). ' +
                'Your current connection is not secure.';
        }
        if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            return 'Your browser does not support media devices (camera/microphone). ' +
                'Please use a modern browser.';
        }
        return null; // ok
    }

    /**
    * Graceful media acquisition with progressive fallback:
    * video call: try {audio+video} → {audio only} → null (receive-only)
    * voice call: try {audio} → null (receive-only)
    * Returns { stream: MediaStream|null, hasAudio: bool, hasVideo: bool }
    */
    function buildAudioConstraints() {
        var c = {
            echoCancellation: voiceQuality.echoCancellation,
            noiseSuppression: voiceQuality.noiseSuppression,
            autoGainControl: voiceQuality.autoGainControl
        };
        if (selectedAudioInput) c.deviceId = { exact: selectedAudioInput };
        return c;
    }

    async function requestMediaGraceful(callType) {
        // Check basic support first
        if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            return { stream: null, hasAudio: false, hasVideo: false };
        }

        var audioConstraints = buildAudioConstraints();

        if (callType === 'video') {
            // Cap resolution at 720p — frameRate is intentionally omitted because
            // some cameras (virtual/fixed-rate) fail getUserMedia with an OverconstrainedError
            // when a max frameRate is specified; the bitrate cap in applyBitrate handles bandwidth.
            var videoConstraints = {
                width: { max: 1280 },
                height: { max: 720 }
            };
            if (selectedVideoInput) videoConstraints.deviceId = { exact: selectedVideoInput };

            // Try video + audio
            try {
                var stream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints, video: videoConstraints });
                return { stream: stream, hasAudio: true, hasVideo: true };
            } catch (e) {
                console.warn('Could not get video+audio, trying audio only:', e.name);
            }
            // Try audio only
            try {
                var audioStream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints });
                return { stream: audioStream, hasAudio: true, hasVideo: false };
            } catch (e2) {
                console.warn('Could not get audio, will be receive-only:', e2.name);
            }
            // Receive-only
            return { stream: null, hasAudio: false, hasVideo: false };
        } else {
            // Voice call: try audio
            try {
                var voiceStream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints });
                return { stream: voiceStream, hasAudio: true, hasVideo: false };
            } catch (e) {
                console.warn('Could not get audio, will be receive-only:', e.name);
            }
            // Receive-only
            return { stream: null, hasAudio: false, hasVideo: false };
        }
    }

    // WebSocket Connection 

    function connectWs() {
        if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
            return;
        }
        wsIntentionalClose = false;

        var protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        var url = protocol + '//' + window.location.host + '/api/ws';

        try {
            ws = new WebSocket(url);
        } catch (e) {
            console.error('Failed to create WebSocket:', e);
            scheduleReconnect();
            return;
        }

        ws.onopen = function () {
            //console.log('WS connected' + (isCallTab ? ' (call tab)' : ''));
            if (wsReconnectTimer) {
                clearTimeout(wsReconnectTimer);
                wsReconnectTimer = null;
            }
            // If this is a call tab and WS just connected, kick off the call
            if (isCallTab && callState === 'waiting_ws') {
                callState = 'idle';
                bootCallTab();
            }
        };

        ws.onclose = function () {
            //('WS closed');
            ws = null;
            if (!wsIntentionalClose) {
                // Call tabs should not reconnect once the call is over
                if (isCallTab && callState === 'idle') return;
                scheduleReconnect();
            }
        };

        ws.onerror = function (e) {
            console.error('WS error:', e);
        };

        ws.onmessage = function (event) {
            try {
                var data = JSON.parse(event.data);
                handleSignal(data);
            } catch (e) {
                console.error('Failed to parse WS message:', e);
            }
        };
    }

    function disconnectWs() {
        wsIntentionalClose = true;
        if (wsReconnectTimer) {
            clearTimeout(wsReconnectTimer);
            wsReconnectTimer = null;
        }
        if (ws) {
            ws.close();
            ws = null;
        }
    }

    function scheduleReconnect() {
        if (wsReconnectTimer) return;
        wsReconnectTimer = setTimeout(function () {
            wsReconnectTimer = null;
            connectWs();
        }, 3000);
    }

    function sendSignal(msg) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify(msg));
        } else {
            console.warn('WS not connected, cannot send signal');
        }
    }

    // ICE Configuration 

    async function fetchIceConfig() {
        if (iceConfig) return iceConfig;
        try {
            iceConfig = await api.get('/api/calls/ice-config');
        } catch (e) {
            console.error('Failed to fetch ICE config:', e);
            // Fallback to public STUN only
            iceConfig = {
                ice_servers: [{ urls: ['stun:stun.l.google.com:19302'] }]
            };
        }
        return iceConfig;
    }

    // Call Tab Bootstrap 

    function initCallTab() {
        var parsed = parseCallHash();
        if (!parsed) return;

        isCallTab = true;
        document.title = 'Magnolia - Call';

        // Hide main app UI, show only call overlay
        var appAuth = document.getElementById('app-auth');
        var appMain = document.getElementById('app-main');
        if (appAuth) appAuth.style.display = 'none';
        if (appMain) appMain.style.display = 'none';

        initDom();

        // Connect WS — once connected, bootCallTab will fire
        if (ws && ws.readyState === WebSocket.OPEN) {
            bootCallTab();
        } else {
            callState = 'waiting_ws';
            connectWs();
        }

        // On tab close / navigate away, hang up and disconnect WS cleanly
        window.addEventListener('beforeunload', function () {
            if (currentCallId && callState !== 'idle') {
                sendSignal({ type: 'call_hangup', call_id: currentCallId });
            }
            cleanup();
            disconnectWs(); // prevent auto-reconnect during tab close
        });
    }

    async function bootCallTab() {
        var parsed = parseCallHash();
        if (!parsed) return;

        await fetchIceConfig();

        // Start E2E worker early so the key pair is ready before first offer/answer
        if (E2E_SUPPORTED) {
            //console.log('[E2E] Creating call-e2e-worker.js Worker');
            callWorker = new Worker('/js/call-e2e-worker.js');
            callWorker.onerror = function (e) {
                console.error('[E2E] Worker load/runtime error:', e.message, 'file:', e.filename, 'line:', e.lineno);
            };
            callWorker.onmessage = function (event) {
                var msg = event.data;
                //console.log('[E2E] Worker message received:', msg.type, msg);
                if (msg.type === 'public_key') {
                    callWorkerPublicKey = msg.bytes;
                    //console.log('[E2E] Worker keygen OK — public key bytes:', callWorkerPublicKey.length);
                    // Flush any targets that were queued before the key was ready
                    pendingKeyTargets.forEach(function (uid) { sendKeyExchange(uid); });
                    pendingKeyTargets = [];
                } else if (msg.type === 'key_ready') {
                    //console.log('[E2E] Frame encryption ready for peer', msg.peerId);
                    if (callStatusText && !callStatusText.textContent.startsWith('\uD83D\uDD12')) {
                        callStatusText.title = 'End-to-end encrypted';
                        callStatusText.textContent = '\uD83D\uDD12 ' + callStatusText.textContent;
                    }
                } else if (msg.type === 'key_error') {
                    //console.warn('[E2E] Key exchange failed for peer', msg.peerId, ':', msg.reason, '— DTLS-SRTP only');
                } else if (msg.type === 'error') {
                    //console.warn('[E2E] Worker error:', msg.reason);
                } else if (msg.type === 'log') {
                    //console.log('[E2E Worker]', msg.msg);
                } else if (msg.type === 'transform_registered') {
                    //console.log('[E2E] Worker reports transform handlers registered via:', msg.methods.join(', '));
                }
            };
            //console.log('[E2E] Sending init to worker');
            callWorker.postMessage({ type: 'init' });
        }

        var media = await requestMediaGraceful(parsed.callType);
        localStream = media.stream;
        pushLocalStreamToPeerConnections();

        currentCallType = parsed.callType;
        currentConversationId = parsed.conversationId;

        // Show camera toggle based on call type (even without local video, user may get camera later)
        var showCamera = parsed.callType === 'video';

        if (parsed.mode === 'out') {
            // Outgoing call
            isCallInitiator = true;
            callState = 'outgoing';
            showCallOverlay('Calling...', showCamera);
            attachLocalStream();
            updateMediaStatusUI(media);

            hasJoinedCall = true;
            sendSignal({
                type: 'call_initiate',
                conversation_id: parsed.conversationId,
                call_type: parsed.callType
            });
        } else if (parsed.mode === 'in') {
            // Incoming call — accept it
            currentCallId = parsed.callId;
            callState = 'connecting';
            showCallOverlay('Connecting...', showCamera);
            attachLocalStream();
            updateMediaStatusUI(media);

            hasJoinedCall = true;
            sendSignal({ type: 'call_accept', call_id: parsed.callId });
        } else if (parsed.mode === 'join') {
            // Join an ongoing active call
            currentCallId = parsed.callId;
            callState = 'connecting';
            showCallOverlay('Joining...', showCamera);
            attachLocalStream();
            updateMediaStatusUI(media);

            hasJoinedCall = true;
            sendSignal({ type: 'call_join', call_id: parsed.callId });
        }
    }

    function updateMediaStatusUI(media) {
        if (!media.stream) {
            updateCallStatus((callState === 'outgoing' ? 'Calling' : 'Connecting') + ' (receive-only)...');
        } else if (currentCallType === 'video' && !media.hasVideo) {
            updateCallStatus((callState === 'outgoing' ? 'Calling' : 'Connecting') + ' (audio only)...');
        }
    }

    // Main Tab: Call Initiation (opens new tab) 

    function startCall(conversationId, callType) {
        if (isCallTab) return;

        var mediaErr = checkMediaSupport();
        if (mediaErr) { alert(mediaErr); return; }

        // Open call in a new tab
        var callUrl = window.location.origin + window.location.pathname + '#call/out/' + conversationId + '/' + callType;
        window.open(callUrl, '_blank');
    }

    function joinCall(callId, callType, conversationId) {
        if (isCallTab) return;

        var mediaErr = checkMediaSupport();
        if (mediaErr) { alert(mediaErr); return; }

        // Open call in a new tab with join mode
        var callUrl = window.location.origin + window.location.pathname +
            '#call/join/' + callId + '/' + callType + '/' + conversationId;
        window.open(callUrl, '_blank');
    }

    // Main Tab: Incoming Call Handling 

    function handleIncomingCall(data) {
        //console.log('[calling] handleIncomingCall:', JSON.stringify(data), 'isCallTab:', isCallTab, 'callState:', callState);
        if (isCallTab) {
            //console.log('[calling] Ignoring call_incoming on call tab');
            return;
        }

        if (callState !== 'idle') {
            //console.log('[calling] Busy — sending call_busy for', data.call_id);
            sendSignal({ type: 'call_busy', call_id: data.call_id });
            return;
        }

        incomingCallData = data;
        callState = 'incoming';
        //console.log('[calling] Showing incoming call banner from', data.caller_name || 'Unknown');
        showIncomingBanner(data.caller_name || 'Unknown', data.call_type);
    }

    function acceptCall() {
        if (!incomingCallData) return;

        var data = incomingCallData;
        incomingCallData = null;
        hideIncomingBanner();
        callState = 'idle'; // reset main tab state

        // Open call in a new tab
        var callUrl = window.location.origin + window.location.pathname +
            '#call/in/' + data.call_id + '/' + data.call_type + '/' + data.conversation_id;
        window.open(callUrl, '_blank');
    }

    function rejectCall() {
        if (!incomingCallData) return;
        sendSignal({ type: 'call_reject', call_id: incomingCallData.call_id });
        incomingCallData = null;
        hideIncomingBanner();
        callState = 'idle';
    }

    // WebRTC Peer Connection Management 

    function createPeerConnection(remoteUserId) {
        var config = { iceServers: iceConfig ? iceConfig.ice_servers : [{ urls: ['stun:stun.l.google.com:19302'] }] };
        /*console.log('[WebRTC] createPeerConnection for', remoteUserId,
            'localStream:', localStream ? ('tracks=' + localStream.getTracks().length) : 'null',
            'iceServers:', JSON.stringify(config.iceServers));*/
        var pc = new RTCPeerConnection(config);

        // Add local tracks if we have them
        if (localStream) {
            localStream.getTracks().forEach(function (track) {
                //('[WebRTC] Adding local track:', track.kind, track.id, 'enabled:', track.enabled);
                var sender = pc.addTrack(track, localStream);
                if (callWorker && E2E_SUPPORTED) {
                    try {
                        sender.transform = new RTCRtpScriptTransform(callWorker, { operation: 'encrypt', peerId: remoteUserId, trackKind: track.kind });
                        //console.log('[E2E] Encrypt transform set on sender for track', track.kind, '— sender.transform:', sender.transform);
                        var tc = pc.getTransceivers().find(function (t) { return t.sender === sender; });
                        if (tc) {
                            tc.receiver.transform = new RTCRtpScriptTransform(callWorker, { operation: 'decrypt', peerId: remoteUserId, trackKind: track.kind });
                            //console.log('[E2E] Decrypt transform set on receiver for track', track.kind);
                        }
                    } catch (e) {
                        console.error('[E2E] Failed to set transforms on track sender:', e);
                    }
                }
            });
        }

        // Ensure we can RECEIVE media even when we don't send it.
        // Use sendrecv even without local track so the remote side sees sendrecv in the SDP,
        // which helps webrtc-rs properly negotiate the connection.
        var senderKinds = {};
        pc.getSenders().forEach(function (s) {
            if (s.track) senderKinds[s.track.kind] = true;
        });
        if (!senderKinds.audio) {
            //console.log('[WebRTC] No local audio track — adding sendrecv transceiver');
            var audioTc = pc.addTransceiver('audio', { direction: 'sendrecv' });
            if (callWorker && E2E_SUPPORTED) {
                try {
                    audioTc.sender.transform = new RTCRtpScriptTransform(callWorker, { operation: 'encrypt', peerId: remoteUserId, trackKind: 'audio' });
                    //console.log('[E2E] Encrypt transform set on audio transceiver sender — transform:', audioTc.sender.transform);
                    audioTc.receiver.transform = new RTCRtpScriptTransform(callWorker, { operation: 'decrypt', peerId: remoteUserId, trackKind: 'audio' });
                    //console.log('[E2E] Decrypt transform set on audio transceiver receiver');
                } catch (e) {
                    console.error('[E2E] Failed to set transforms on audio transceiver:', e);
                }
            }
        }
        if (currentCallType === 'video' && !senderKinds.video) {
            // Use a real (black) track so the SSRC is included in the SDP offer/answer.
            // A bare addTransceiver without a track omits the SSRC, which prevents
            // replaceTrack (screenshare) from actually transmitting to the remote peer.
            var videoSender = pc.addTrack(getBlackVideoTrack());
            if (callWorker && E2E_SUPPORTED) {
                try {
                    videoSender.transform = new RTCRtpScriptTransform(callWorker, { operation: 'encrypt', peerId: remoteUserId, trackKind: 'video' });
                    //console.log('[E2E] Encrypt transform set on video sender — transform:', videoSender.transform);
                    var videoTc = pc.getTransceivers().find(function (t) { return t.sender === videoSender; });
                    if (videoTc) {
                        videoTc.receiver.transform = new RTCRtpScriptTransform(callWorker, { operation: 'decrypt', peerId: remoteUserId, trackKind: 'video' });
                        //console.log('[E2E] Decrypt transform set on video transceiver receiver');
                    }
                } catch (e) {
                    console.error('[E2E] Failed to set transforms on video sender:', e);
                }
            }
        }

        // ICE candidates
        pc.onicecandidate = function (event) {
            if (event.candidate) {
                //console.log('[WebRTC] Local ICE candidate:', event.candidate.candidate);
                sendSignal({
                    type: 'ice_candidate',
                    call_id: currentCallId,
                    target_user_id: remoteUserId,
                    candidate: event.candidate.toJSON()
                });
            } else {
                //console.log('[WebRTC] ICE gathering complete (null candidate)');
            }
        };

        // ICE connection state (transport-level connectivity)
        pc.oniceconnectionstatechange = function () {
            //console.log('[WebRTC] ICE connection state:', pc.iceConnectionState, 'for', remoteUserId);
        };

        // ICE gathering state
        pc.onicegatheringstatechange = function () {
            //console.log('[WebRTC] ICE gathering state:', pc.iceGatheringState, 'for', remoteUserId);
        };

        // Remote tracks — fires once per track (audio, then video)
        pc.ontrack = function (event) {
            /*console.log('[WebRTC] ontrack:', event.track.kind, 'from', remoteUserId,
                'streams:', event.streams ? event.streams.length : 0);*/

            // First track: create our stream + DOM element
            if (!remoteStreams[remoteUserId]) {
                remoteStreams[remoteUserId] = (event.streams && event.streams[0])
                    ? event.streams[0]
                    : new MediaStream();
                addRemoteStreamElement(remoteUserId);
            }

            var ourStream = remoteStreams[remoteUserId];

            // Ensure this track is in our stream
            if (!ourStream.getTracks().some(function (t) { return t.id === event.track.id; })) {
                ourStream.addTrack(event.track);
            }

            var mediaEl = document.getElementById('remote-video-' + remoteUserId);
            if (mediaEl) {
                if (mediaEl.srcObject !== ourStream) {
                    // First time (or stream changed): assign and start playback.
                    // Subsequent ontrack events add tracks to the already-attached stream;
                    // the browser renders new tracks automatically — no reload needed.
                    mediaEl.srcObject = ourStream;
                    playRemoteMedia(mediaEl);
                }
            }
        };

        // Connection state (includes ICE + DTLS)
        pc.onconnectionstatechange = function () {
            //console.log('[WebRTC] Connection state:', pc.connectionState, 'for', remoteUserId);
            if (pc.connectionState === 'connected') {
                onPeerConnected();
            } else if (pc.connectionState === 'disconnected') {
                // Transient — ICE may recover on its own; just update UI
                updateCallStatus('Reconnecting...');
            } else if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
                console.error('[WebRTC] Connection', pc.connectionState, 'for', remoteUserId, '— ending call');
                endCall();
            }
        };

        // Signaling state changes
        pc.onsignalingstatechange = function () {
            //console.log('[WebRTC] Signaling state:', pc.signalingState, 'for', remoteUserId);
        };

        peerConnections[remoteUserId] = pc;
        return pc;
    }

    // Receive a peer's E2E public key and derive the shared AES-GCM key
    function handleKeyExchange(data) {
        if (!callWorker || !data.public_key) return;
        //console.log('[E2E] Received key_exchange from', data.from_user_id);
        var peerBytes = Uint8Array.from(atob(data.public_key), function (c) { return c.charCodeAt(0); });
        callWorker.postMessage({ type: 'peer_key', peerId: data.from_user_id, bytes: peerBytes });
        // Send our key back if we haven't already (handles whichever side sends first)
        if (callWorkerPublicKey) {
            sendKeyExchange(data.from_user_id);
        } else {
            pendingKeyTargets.push(data.from_user_id);
        }
    }

    // Send our ephemeral E2E public key to a peer via the signaling channel
    function sendKeyExchange(targetUserId) {
        if (!callWorkerPublicKey || !currentCallId) return;
        if (keySentTo[targetUserId]) return; // already sent
        keySentTo[targetUserId] = true;
        var b64 = btoa(String.fromCharCode.apply(null, callWorkerPublicKey));
        sendSignal({
            type: 'key_exchange',
            call_id: currentCallId,
            target_user_id: targetUserId,
            public_key: b64
        });
        //console.log('[E2E] Sent key_exchange to', targetUserId);
    }

    async function createAndSendOffer(remoteUserId) {
        var pc = peerConnections[remoteUserId] || createPeerConnection(remoteUserId);
        try {
            var offer = await pc.createOffer();
            await pc.setLocalDescription(offer);
            sendSignal({
                type: 'sdp_offer',
                call_id: currentCallId,
                target_user_id: remoteUserId,
                sdp: offer.sdp
            });
            // Piggyback key exchange alongside the offer
            if (callWorkerPublicKey) {
                sendKeyExchange(remoteUserId);
            } else {
                pendingKeyTargets.push(remoteUserId);
            }
        } catch (e) {
            console.error('Failed to create offer for ' + remoteUserId + ':', e);
        }
    }

    async function handleSdpOffer(data) {
        //console.log('[WebRTC] handleSdpOffer from', data.from_user_id, 'sdp length:', data.sdp ? data.sdp.length : 0);
        var pc = peerConnections[data.from_user_id] || createPeerConnection(data.from_user_id);
        try {
            await pc.setRemoteDescription({ type: 'offer', sdp: data.sdp });
            //console.log('[WebRTC] Remote description (offer) set successfully');
            await flushPendingIceCandidates(data.from_user_id);
            // For video calls, keep the video transceiver sendrecv even when we have no
            // local camera. Without this Chrome downgrades to recvonly in the answer,
            // which permanently prevents the PC side from screensharing into the call.
            if (currentCallType === 'video') {
                pc.getTransceivers().forEach(function (tc) {
                    if (tc.direction !== 'stopped') {
                        tc.direction = 'sendrecv';
                    }
                });
            }
            var answer = await pc.createAnswer();
            //console.log('[WebRTC] Answer created, direction check:', answer.sdp.indexOf('sendrecv') >= 0 ? 'sendrecv' : (answer.sdp.indexOf('recvonly') >= 0 ? 'recvonly' : 'other'));
            await pc.setLocalDescription(answer);
            //console.log('[WebRTC] Local description (answer) set, sending to', data.from_user_id);
            sendSignal({
                type: 'sdp_answer',
                call_id: currentCallId,
                target_user_id: data.from_user_id,
                sdp: answer.sdp
            });
            // Piggyback key exchange alongside the answer
            if (callWorkerPublicKey) {
                sendKeyExchange(data.from_user_id);
            } else {
                pendingKeyTargets.push(data.from_user_id);
            }
        } catch (e) {
            console.error('Failed to handle SDP offer from ' + data.from_user_id + ':', e);
        }
    }

    async function handleSdpAnswer(data) {
        var pc = peerConnections[data.from_user_id];
        if (pc) {
            try {
                await pc.setRemoteDescription({ type: 'answer', sdp: data.sdp });
                await flushPendingIceCandidates(data.from_user_id);
            } catch (e) {
                console.error('Failed to handle SDP answer from ' + data.from_user_id + ':', e);
            }
        }
    }

    async function handleIceCandidate(data) {
        var pc = peerConnections[data.from_user_id];
        // Buffer if no peer connection yet (ICE arrives before SDP offer on receiver side)
        // or if the peer connection exists but remote description isn't set yet.
        if (!pc || !pc.remoteDescription) {
            var reason = !pc ? 'no peer connection yet' : 'remote description not set yet';
            //console.log('[WebRTC] Buffering ICE candidate from', data.from_user_id, '—', reason);
            if (!pendingIceCandidates[data.from_user_id]) pendingIceCandidates[data.from_user_id] = [];
            pendingIceCandidates[data.from_user_id].push(data.candidate);
            return;
        }
        try {
            /*console.log('[WebRTC] Adding remote ICE candidate from', data.from_user_id + ':',
                data.candidate ? data.candidate.candidate : 'null');*/
            await pc.addIceCandidate(data.candidate);
            //console.log('[WebRTC] ICE candidate added OK');
        } catch (e) {
            console.error('Failed to add ICE candidate from ' + data.from_user_id + ':', e);
        }
    }

    async function flushPendingIceCandidates(userId) {
        var pc = peerConnections[userId];
        var buffered = pendingIceCandidates[userId];
        if (!pc || !buffered || buffered.length === 0) return;
        delete pendingIceCandidates[userId];
        //console.log('[WebRTC] Flushing', buffered.length, 'buffered ICE candidate(s) for', userId);
        for (var i = 0; i < buffered.length; i++) {
            try {
                await pc.addIceCandidate(buffered[i]);
                //console.log('[WebRTC] Buffered ICE candidate added OK');
            } catch (e) {
                console.error('[WebRTC] Failed to add buffered ICE candidate:', e);
            }
        }
    }

    function closePeerConnection(userId) {
        if (peerConnections[userId]) {
            peerConnections[userId].close();
            delete peerConnections[userId];
        }
        if (remoteStreams[userId]) {
            delete remoteStreams[userId];
        }
        removeRemoteStreamElement(userId);
    }

    function onPeerConnected() {
        if (callState !== 'connected') {
            callState = 'connected';
            callStartTime = Date.now();
            if (!autoplayRecoveryPending) updateCallStatus('Connected');
            startCallTimer();
            // Apply birthrate setting now that connection is established
            applyBitrate();
        }
    }

    // Call Controls 

    function toggleMute() {
        if (!localStream) return;
        var audioTracks = localStream.getAudioTracks();
        if (audioTracks.length > 0) {
            audioTracks[0].enabled = !audioTracks[0].enabled;
            if (btnToggleMute) {
                btnToggleMute.classList.toggle('active', !audioTracks[0].enabled);
                btnToggleMute.querySelector('.call-btn-label').textContent = audioTracks[0].enabled ? 'Mute' : 'Unmute';
            }
        }
    }

    function toggleCamera() {
        if (!localStream) return;
        var videoTracks = localStream.getVideoTracks();
        if (videoTracks.length > 0) {
            videoTracks[0].enabled = !videoTracks[0].enabled;
            if (btnToggleCamera) {
                btnToggleCamera.classList.toggle('active', !videoTracks[0].enabled);
                btnToggleCamera.querySelector('.call-btn-label').textContent = videoTracks[0].enabled ? 'Camera' : 'Cam Off';
            }
        }
    }

    // Voice Quality Controls 

    function toggleVoiceQualityPanel() {
        if (!voiceQualityPanel) return;
        var showing = voiceQualityPanel.style.display !== 'none';
        voiceQualityPanel.style.display = showing ? 'none' : '';
        if (!showing) loadDevices(); // refresh device list each time the panel opens
        if (btnVoiceQuality) btnVoiceQuality.classList.toggle('active', !showing);
    }

    function setPreset(preset) {
        if (!PRESETS[preset]) return;
        voiceQuality.preset = preset;

        // Update UI buttons
        document.querySelectorAll('.vq-preset').forEach(function (btn) {
            btn.classList.toggle('active', btn.dataset.preset === preset);
        });

        // Presets only affect send bitrate — no need to reacquire the mic track
        applyBitrate();
    }

    /**
    * Re-acquire audio from getUserMedia with current voiceQuality settings,
    * then replace the track in every peer connection sender.
    * This is necessary because echoCancellation/noiseSuppression/autoGainControl
    * are locked at capture time and cannot be changed via applyConstraints().
    *
    * Safety: the old track is only stopped AFTER all sender replacements succeed.
    * If getUserMedia or all replacements fail, the old track stays active.
    */
    async function reacquireAudioTrack() {
        if (!localStream) return;
        var oldAudioTrack = localStream.getAudioTracks()[0];
        if (!oldAudioTrack) return;

        var newStream;
        try {
            newStream = await navigator.mediaDevices.getUserMedia({ audio: buildAudioConstraints() });
        } catch (e) {
            console.warn('Could not re-acquire audio with new constraints:', e);
            return; // keep old track
        }

        var newAudioTrack = newStream.getAudioTracks()[0];
        if (!newAudioTrack) return;

        // Preserve mute state
        var wasMuted = !oldAudioTrack.enabled;
        if (wasMuted) newAudioTrack.enabled = false;

        // Replace the track in every peer connection sender — await all before swapping
        var replaceResults = [];
        Object.keys(peerConnections).forEach(function (userId) {
            var pc = peerConnections[userId];
            pc.getSenders().forEach(function (sender) {
                if (sender.track && sender.track.kind === 'audio') {
                    replaceResults.push(
                        sender.replaceTrack(newAudioTrack).then(function () {
                            return true;
                        }).catch(function (e) {
                            console.warn('Could not replace audio track for ' + userId + ':', e);
                            return false;
                        })
                    );
                }
            });
        });

        var results = await Promise.all(replaceResults);
        var anySucceeded = results.length === 0 || results.some(function (r) { return r; });

        if (!anySucceeded) {
            // All sender replacements failed — revert, keep old track
            console.warn('All track replacements failed, keeping old track');
            newAudioTrack.stop();
            return;
        }

        // Success — now safe to swap in localStream and stop old track
        localStream.removeTrack(oldAudioTrack);
        localStream.addTrack(newAudioTrack);
        oldAudioTrack.stop();
    }

    function applyBitrate() {
        var preset = PRESETS[voiceQuality.preset];
        if (!preset) return;

        Object.keys(peerConnections).forEach(function (userId) {
            var pc = peerConnections[userId];
            pc.getSenders().forEach(function (sender) {
                if (!sender.track) return;
                try {
                    var params = sender.getParameters();
                    if (!params.encodings || params.encodings.length === 0) return;
                    if (sender.track.kind === 'audio') {
                        params.encodings[0].maxBitrate = preset.maxBitrate;
                    } else if (sender.track.kind === 'video') {
                        // Cap video bitrate to keep E2E frame sizes manageable
                        params.encodings[0].maxBitrate = 1500000; // 1.5 Mbps for 720p
                        params.encodings[0].maxFramerate = 30;
                    }
                    sender.setParameters(params).catch(function (e) {
                        console.warn('Could not set bitrate for ' + userId + ':', e);
                    });
                } catch (e) {
                    console.warn('Could not get sender params for ' + userId + ':', e);
                }
            });
        });
    }

    function onVoiceSettingChange() {
        var echoEl = document.getElementById('vq-echo');
        var noiseEl = document.getElementById('vq-noise');
        var gainEl = document.getElementById('vq-gain');

        if (echoEl) voiceQuality.echoCancellation = echoEl.checked;
        if (noiseEl) voiceQuality.noiseSuppression = noiseEl.checked;
        if (gainEl) voiceQuality.autoGainControl = gainEl.checked;

        reacquireAudioTrack();
    }

    // Receiver (Playback) Controls 

    function setReceiverVolume(vol) {
        receiverVolume = Math.max(0, Math.min(1, vol));
        // Apply to all remote audio/video elements
        Object.keys(remoteStreams).forEach(function (userId) {
            var el = document.getElementById('remote-video-' + userId);
            if (el) el.volume = receiverVolume;
        });
        var label = document.getElementById('vq-volume-label');
        if (label) label.textContent = Math.round(receiverVolume * 100) + '%';
    }

    function initVoiceQualityPanel() {
        voiceQualityPanel = document.getElementById('voice-quality-panel');
        btnVoiceQuality = document.getElementById('btn-voice-quality');

        if (btnVoiceQuality) {
            btnVoiceQuality.onclick = toggleVoiceQualityPanel;
        }

        var closeBtn = document.getElementById('btn-vq-close');
        if (closeBtn) {
            closeBtn.onclick = function () {
                if (voiceQualityPanel) voiceQualityPanel.style.display = 'none';
                if (btnVoiceQuality) btnVoiceQuality.classList.remove('active');
            };
        }

        // Preset buttons
        document.querySelectorAll('.vq-preset').forEach(function (btn) {
            btn.onclick = function () { setPreset(btn.dataset.preset); };
        });

        // Toggle checkboxes (sender)
        var echoEl = document.getElementById('vq-echo');
        var noiseEl = document.getElementById('vq-noise');
        var gainEl = document.getElementById('vq-gain');
        if (echoEl) echoEl.onchange = onVoiceSettingChange;
        if (noiseEl) noiseEl.onchange = onVoiceSettingChange;
        if (gainEl) gainEl.onchange = onVoiceSettingChange;

        // Volume slider (receiver)
        var volumeSlider = document.getElementById('vq-volume');
        if (volumeSlider) {
            volumeSlider.oninput = function () {
                setReceiverVolume(parseInt(volumeSlider.value, 10) / 100);
            };
        }

        // Device selects
        var audioInputSel = document.getElementById('vq-audio-input');
        var audioOutputSel = document.getElementById('vq-audio-output');
        var videoInputSel = document.getElementById('vq-video-input');
        if (audioInputSel) audioInputSel.onchange = function () { setAudioInput(audioInputSel.value); };
        if (audioOutputSel) audioOutputSel.onchange = function () { setAudioOutput(audioOutputSel.value); };
        if (videoInputSel) videoInputSel.onchange = function () { setVideoInput(videoInputSel.value); };

        // Hide camera device row for voice-only calls
        var videoRow = document.getElementById('vq-video-input-row');
        if (videoRow) videoRow.style.display = currentCallType === 'video' ? '' : 'none';
    }

    // Initiator Controls 

    function kickParticipant(userId) {
        if (!currentCallId) return;
        sendSignal({ type: 'call_kick', call_id: currentCallId, target_user_id: userId });
    }

    /**
    * Return (creating if needed) a 2×2 black canvas video track.
    * Using a real track in the offer/answer puts the sender SSRC into the SDP,
    * which is required for replaceTrack (screenshare) to actually transmit frames.
    * The canvas runs at 0 fps after the first frame, so bandwidth cost is negligible.
    */
    function getBlackVideoTrack() {
        if (blackVideoTrack && blackVideoTrack.readyState === 'live') {
            return blackVideoTrack;
        }
        var canvas = document.createElement('canvas');
        canvas.width = 2;
        canvas.height = 2;
        blackVideoTrack = canvas.captureStream(30).getVideoTracks()[0];
        return blackVideoTrack;
    }

    // Per-User Volume 

    function setRemoteVolume(userId, vol) {
        vol = Math.max(0, Math.min(1, vol));
        remoteVolumes[userId] = vol;
        var el = document.getElementById('remote-video-' + userId);
        if (el) el.volume = vol;
    }

    // Screen Sharing 

    /**
    * Start playback on a remote media element.
    *
    * The element carries autoplay+muted attributes so Chrome's autoplay policy
    * allows it unconditionally. Once the 'playing' event fires we unmute —
    * toggling .muted on an already-playing element does NOT re-trigger policy.
    *
    * Belt-and-suspenders: if the autoplay attribute didn't fire (Chrome
    * defers it on srcObject assignment sometimes) we call play() explicitly.
    * A 2 sec timeout shows the tap-to-play overlay as a last resort.
    * 
    * I don't even want to test the other browsers... Why can't this be simple??
    */
    function playRemoteMedia(mediaEl) {
        /*console.log('[WebRTC] playRemoteMedia:', mediaEl.id,
            'paused:', mediaEl.paused, 'muted:', mediaEl.muted,
            'tracks:', mediaEl.srcObject
            ? mediaEl.srcObject.getTracks().map(function (t) { return t.kind + '/' + t.readyState; })
            : 'null');*/

        if (!mediaEl.paused) {
            // Already playing (autoplay attr fired before we got here) — just unmute.
            if (mediaEl.muted) {
                //console.log('[WebRTC] Already playing muted — unmuting', mediaEl.id);
                mediaEl.muted = false;
            }
            return;
        }

        // Diagnostic listeners — for logging only, not for unmuting.
        mediaEl.addEventListener('playing', function () {
            //console.log('[WebRTC] playing event for', mediaEl.id, '(first frame rendered)');
        }, { once: true });
        mediaEl.addEventListener('timeupdate', function () {
            //console.log('[WebRTC] timeupdate (media flowing) for', mediaEl.id);
        }, { once: true });

        // Explicit muted play() — Chrome always allows muted autoplay.
        // Unmute immediately when play() resolves: the browser has accepted the stream
        // so autoplay policy is satisfied. Don't wait for 'playing' (which requires a
        // rendered video frame and won't fire if the video track has no data yet).
        mediaEl.muted = true;
        var playPromise = mediaEl.play();
        if (playPromise) {
            playPromise
                .then(function () {
                    //console.log('[WebRTC] play() resolved for', mediaEl.id, '— unmuting');
                    mediaEl.muted = false;
                })
                .catch(function (e) {
                    console.warn('[WebRTC] play() rejected:', e.name, 'for', mediaEl.id);
                    if (e.name === 'AbortError') {
                        // srcObject not loaded yet — retry once metadata is ready.
                        mediaEl.addEventListener('loadedmetadata', function () {
                            playRemoteMedia(mediaEl);
                        }, { once: true });
                    } else {
                        showPlayBlockedOverlay(mediaEl);
                    }
                });
        }

        // Fallback: if play() is still pending after 2 s, the stream has no data.
        setTimeout(function () {
            if (mediaEl.paused) {
                console.warn('[WebRTC] Still paused after 2 s for', mediaEl.id, '— showing overlay');
                showPlayBlockedOverlay(mediaEl);
            }
        }, 2000);
    }

    /**
    * Resume all paused remote <video>/<audio> elements and hide their overlays.
    * Called both from the document-level click recovery and from pre-share resume.
    */
    function resumeAllPausedStreams() {
        document.querySelectorAll('.remote-stream-item').forEach(function (container) {
            var mediaEl = container.querySelector('video, audio');
            if (!mediaEl) return;
            var ov = container.querySelector('.play-blocked-overlay');
            if (ov) ov.style.display = 'none';
            if (mediaEl.paused) {
                mediaEl.muted = true;
                mediaEl.play().then(function () {
                    mediaEl.muted = false;
                }).catch(function (e) {
                    console.warn('[WebRTC] resumeAllPausedStreams play() failed:', e.name);
                });
            } else if (mediaEl.muted) {
                mediaEl.muted = false;
            }
        });
    }

    // Whether we already registered the document-level autoplay recovery listener.
    var autoplayRecoveryPending = false;

    /** Show a full-cover "tap to play" overlay over the remote video tile,
    * and register a one-shot document click listener so ANY interaction
    * (mute button, end-call, etc.) resumes all paused streams. */
    function showPlayBlockedOverlay(mediaEl) {
        console.warn('[WebRTC] Autoplay blocked for', mediaEl.id, '— showing tap-to-play overlay');
        var container = mediaEl.closest('.remote-stream-item');
        if (container) {
            var overlay = container.querySelector('.play-blocked-overlay');
            if (!overlay) {
                overlay = document.createElement('div');
                overlay.className = 'play-blocked-overlay';
                overlay.innerHTML = '<span>▶ Tap to play</span>';
                container.appendChild(overlay);
            }
            overlay.style.display = 'flex';
            overlay.onclick = function () {
                resumeAllPausedStreams();
            };
        }

        // Register a document-wide one-shot click listener so the user doesn't
        // have to find and click the small overlay — any button press works.
        if (!autoplayRecoveryPending) {
            autoplayRecoveryPending = true;
            updateCallStatus('Click anywhere to start audio/video');
            document.addEventListener('click', function recoverAutoplay() {
                autoplayRecoveryPending = false;
                document.removeEventListener('click', recoverAutoplay);
                resumeAllPausedStreams();
                // Restore normal status text
                if (callState === 'connected') updateCallStatus('Connected');
            }, { capture: true, once: true });
        }
    }

    /**
    * Return all video senders for a peer connection.
    * Checks transceivers by receiver track kind so that senders whose track is
    * currently null (no local camera) are included — otherwise replaceTrack for
    * screen share would silently skip those connections.
    */
    function getVideoSenders(pc) {
        var senders = [];
        var seen = new Set();
        pc.getTransceivers().forEach(function (tc) {
            if (tc.receiver && tc.receiver.track && tc.receiver.track.kind === 'video') {
                senders.push(tc.sender);
                seen.add(tc.sender);
            }
        });
        // Fallback: directly-tracked video senders (covers sendonly transceivers where
        // Chrome may leave receiver.track null, and browsers with partial transceiver support)
        pc.getSenders().forEach(function (s) {
            if (!seen.has(s) && s.track && s.track.kind === 'video') {
                senders.push(s);
            }
        });
        /*console.log('[ScreenShare] getVideoSenders found', senders.length, 'sender(s)',
            senders.map(function (s) { return s.track ? s.track.kind : 'null-track'; }));*/
        return senders;
    }

    async function toggleScreenShare() {
        if (isScreenSharing) {
            stopScreenShare();
            return;
        }
        if (!navigator.mediaDevices || !navigator.mediaDevices.getDisplayMedia) {
            alert('Screen sharing is not supported in this browser.');
            return;
        }
        try {
            // Step 1: resume paused remote elements 
            // Do this BEFORE getDisplayMedia, which consumes the user-gesture.
            // If play() was previously blocked by autoplay policy (NotAllowedError),
            // this button-click gesture is our only chance to resume without an extra tap.
            autoplayRecoveryPending = false;
            resumeAllPausedStreams();

            // Step 2: capture screen 
            screenStream = await navigator.mediaDevices.getDisplayMedia({ video: true });
            var screenTrack = screenStream.getVideoTracks()[0];

            // Step 3: replace video track in all peer connections 
            var promises = [];
            Object.keys(peerConnections).forEach(function (userId) {
                var pc = peerConnections[userId];
                getVideoSenders(pc).forEach(function (sender) {
                    promises.push(sender.replaceTrack(screenTrack).catch(function (e) {
                        console.warn('replaceTrack (screen) failed for ' + userId + ':', e);
                    }));
                });
            });
            await Promise.all(promises);

            // Show screen in local preview
            if (localVideoEl) localVideoEl.srcObject = screenStream;

            isScreenSharing = true;
            if (btnScreenShare) {
                btnScreenShare.classList.add('active');
                var ssLabel = btnScreenShare.querySelector('.call-btn-label');
                if (ssLabel) ssLabel.textContent = 'Stop Share';
            }

            // Auto-stop when the user ends sharing via the browser UI
            screenTrack.onended = function () { stopScreenShare(); };
        } catch (e) {
            if (e.name !== 'NotAllowedError') {
                console.warn('Screen share failed:', e.name, e.message);
            }
            screenStream = null;
        }
    }

    function stopScreenShare() {
        if (!isScreenSharing) return;

        // Restore to camera track, or fall back to the black placeholder so the
        // SSRC stays active and a future screenshare can replace it again.
        var cameraTrack = localStream ? localStream.getVideoTracks()[0] : null;
        if (!cameraTrack) cameraTrack = getBlackVideoTrack();
        var promises = [];
        Object.keys(peerConnections).forEach(function (userId) {
            var pc = peerConnections[userId];
            pc.getSenders().forEach(function (sender) {
                if (sender.track && sender.track.kind === 'video') {
                    promises.push(sender.replaceTrack(cameraTrack || null).catch(function (e) {
                        console.warn('replaceTrack (camera restore) failed for ' + userId + ':', e);
                    }));
                }
            });
        });

        Promise.all(promises).then(function () {
            if (screenStream) {
                screenStream.getTracks().forEach(function (t) { t.stop(); });
                screenStream = null;
            }
            isScreenSharing = false;
            if (localVideoEl) localVideoEl.srcObject = localStream;
            if (btnScreenShare) {
                btnScreenShare.classList.remove('active');
                var ssLabel = btnScreenShare.querySelector('.call-btn-label');
                if (ssLabel) ssLabel.textContent = 'Share';
            }
        });
    }

    // Device Management 

    async function loadDevices() {
        if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) return;
        try {
            var devices = await navigator.mediaDevices.enumerateDevices();
            var audioInputs = devices.filter(function (d) { return d.kind === 'audioinput'; });
            var audioOutputs = devices.filter(function (d) { return d.kind === 'audiooutput'; });
            var videoInputs = devices.filter(function (d) { return d.kind === 'videoinput'; });
            populateDeviceSelect('vq-audio-input', audioInputs, selectedAudioInput);
            populateDeviceSelect('vq-audio-output', audioOutputs, selectedAudioOutput);
            populateDeviceSelect('vq-video-input', videoInputs, selectedVideoInput);
        } catch (e) {
            console.warn('Could not enumerate devices:', e);
        }
    }

    function populateDeviceSelect(selectId, devices, currentId) {
        var el = document.getElementById(selectId);
        if (!el) return;
        el.innerHTML = '';
        devices.forEach(function (d, i) {
            var opt = document.createElement('option');
            opt.value = d.deviceId;
            opt.textContent = d.label || (d.kind + ' ' + (i + 1));
            if (d.deviceId === currentId) opt.selected = true;
            el.appendChild(opt);
        });
        if (devices.length === 0) {
            var opt = document.createElement('option');
            opt.textContent = 'None available';
            el.appendChild(opt);
        }
    }

    async function setAudioOutput(deviceId) {
        selectedAudioOutput = deviceId;
        // setSinkId is Chrome/Edge only
        var setPromises = [];
        Object.keys(remoteStreams).forEach(function (userId) {
            var el = document.getElementById('remote-video-' + userId);
            if (el && el.setSinkId) {
                setPromises.push(el.setSinkId(deviceId).catch(function (e) {
                    console.warn('setSinkId failed for ' + userId + ':', e);
                }));
            }
        });
        await Promise.all(setPromises);
    }

    async function setVideoInput(deviceId) {
        selectedVideoInput = deviceId;
        if (!localStream) return;
        var oldTrack = localStream.getVideoTracks()[0];
        if (!oldTrack) return;
        try {
            var newStream = await navigator.mediaDevices.getUserMedia({
                video: { deviceId: { exact: deviceId } }
            });
            var newTrack = newStream.getVideoTracks()[0];
            if (!newTrack) return;

            var promises = [];
            Object.keys(peerConnections).forEach(function (userId) {
                var pc = peerConnections[userId];
                pc.getSenders().forEach(function (sender) {
                    if (sender.track && sender.track.kind === 'video') {
                        promises.push(sender.replaceTrack(newTrack).catch(function (e) {
                            console.warn('replaceTrack (video input) failed for ' + userId + ':', e);
                        }));
                    }
                });
            });
            await Promise.all(promises);

            localStream.removeTrack(oldTrack);
            localStream.addTrack(newTrack);
            oldTrack.stop();
            if (localVideoEl && !isScreenSharing) localVideoEl.srcObject = localStream;
        } catch (e) {
            console.warn('Could not switch video input:', e);
        }
    }

    async function setAudioInput(deviceId) {
        selectedAudioInput = deviceId;
        await reacquireAudioTrack();
    }

    function endCall() {
        if (currentCallId) {
            sendSignal({ type: 'call_hangup', call_id: currentCallId });
        }
        cleanup();

        if (isCallTab) {
            // Disconnect WS immediately — prevents zombie connections
            disconnectWs();
            document.title = 'Call Ended';
            setTimeout(function () {
                try { window.close(); } catch (e) { /* ignore */ }
            }, 500);
        }
    }

    function cleanup() {
        // Close all peer connections
        Object.keys(peerConnections).forEach(function (userId) {
            peerConnections[userId].close();
        });
        peerConnections = {};
        remoteStreams = {};

        // Stop local media
        if (localStream) {
            localStream.getTracks().forEach(function (t) { t.stop(); });
            localStream = null;
        }

        // Stop screen share if active
        if (isScreenSharing && screenStream) {
            screenStream.getTracks().forEach(function (t) { t.stop(); });
            screenStream = null;
        }

        // Release the black placeholder track
        if (blackVideoTrack) {
            blackVideoTrack.stop();
            blackVideoTrack = null;
        }
        isScreenSharing = false;
        if (btnScreenShare) {
            btnScreenShare.classList.remove('active');
            var ssLabel = btnScreenShare.querySelector('.call-btn-label');
            if (ssLabel) ssLabel.textContent = 'Share';
        }

        // Reset autoplay recovery state
        autoplayRecoveryPending = false;

        // Reset state
        currentCallId = null;
        currentCallType = null;
        currentConversationId = null;
        callState = 'idle';
        callStartTime = null;
        incomingCallData = null;
        isCallInitiator = false;
        participantNames = {};
        remoteVolumes = {};

        // Tear down E2E worker
        if (callWorker) {
            callWorker.terminate();
            callWorker = null;
        }
        callWorkerPublicKey = null;
        pendingKeyTargets = [];
        keySentTo = {};

        if (callTimerInterval) {
            clearInterval(callTimerInterval);
            callTimerInterval = null;
        }

        // Hide UI
        hideCallOverlay();
        hideIncomingBanner();
    }

    // Signal Dispatcher 

    function handleSignal(data) {
        //console.log('[calling] WS signal received:', data.type, isCallTab ? '(call tab)' : '(main tab)');
        switch (data.type) {
            case 'call_incoming':
                handleIncomingCall(data);
                break;

            case 'call_accepted':
                if (!isCallTab) {
                    // Main tab: a call is now active, refresh the ongoing-call lane
                    window.dispatchEvent(new CustomEvent('magnolia:check-active-call'));
                    break;
                }
                // Call-tab: guard against pre-join glare (see hasJoinedCall comment above)
                if (!hasJoinedCall) break;
                if (data.call_id) currentCallId = data.call_id;
                // Create peer connection and send offer to the accepting user
                createAndSendOffer(data.user_id);
                break;

            case 'call_rejected':
                if (isCallTab && callState === 'outgoing') {
                    //console.log('User ' + data.user_id + ' rejected the call');
                    updateCallStatus('Call rejected');
                    setTimeout(endCall, 2000);
                }
                break;

            case 'call_ended':
                if (isCallTab) {
                    cleanup();
                    disconnectWs();
                    // Show ended state briefly then close
                    document.title = 'Call Ended';
                    showCallOverlay('Call ended', false);
                    setTimeout(function () {
                        try { window.close(); } catch (e) { /* ignore */ }
                    }, 2000);
                } else {
                    // Main tab: reset incoming banner and notify UI that call ended
                    if (incomingCallData && incomingCallData.call_id === data.call_id) {
                        incomingCallData = null;
                        hideIncomingBanner();
                        callState = 'idle';
                    }
                    window.dispatchEvent(new CustomEvent('magnolia:check-active-call'));
                }
                break;

            case 'participant_joined':
                if (!isCallTab || !hasJoinedCall) break;
                // Store display name for use in the tile overlay
                if (data.user_name) participantNames[data.user_id] = data.user_name;
                // Group call: new participant joined — send them an offer
                createAndSendOffer(data.user_id);
                break;

            case 'call_kicked':
                if (!isCallTab) break;
                cleanup();
                disconnectWs();
                document.title = 'Call Ended';
                showCallOverlay('You were removed from the call', false);
                setTimeout(function () {
                    try { window.close(); } catch (e) { /* ignore */ }
                }, 2000);
                break;

            case 'participant_left':
                if (!isCallTab) break;
                closePeerConnection(data.user_id);
                break;

            case 'sdp_offer':
                if (!isCallTab) break;
                handleSdpOffer(data);
                break;

            case 'sdp_answer':
                if (!isCallTab) break;
                handleSdpAnswer(data);
                break;

            case 'ice_candidate':
                if (!isCallTab) break;
                handleIceCandidate(data);
                break;

            case 'key_exchange':
                if (!isCallTab) break;
                handleKeyExchange(data);
                break;

            case 'open_call_available':
                // An open/group call started in a conversation — notify main tab UI
                if (!isCallTab) {
                    window.dispatchEvent(new CustomEvent('magnolia:check-active-call', {
                        detail: { conversationId: data.conversation_id }
                    }));
                }
                break;

            case 'global_call_update':
            case 'global_call_offer':
            case 'global_call_answer':
            case 'global_call_ice':
                // Forward to global-call.js via custom event (main tab only)
                if (!isCallTab) {
                    window.dispatchEvent(new CustomEvent('magnolia:signal', { detail: data }));
                }
                break;

            case 'pong':
            case 'ping':
                if (data.type === 'ping') {
                    sendSignal({ type: 'pong' });
                }
                break;

            case 'new_message':
                if (typeof messaging !== 'undefined') {
                    messaging.onNewMessageSignal(data);
                }
                break;

            case 'event':
                if (!isCallTab && typeof events !== 'undefined' && data.event) {
                    events.handleWsEvent(data.event);
                }
                break;

            case 'error':
                console.error('Server error:', data.message);
                break;

            default:
                console.warn('Unknown signal type:', data.type);
        }
    }

    // UI Management 

    function initDom() {
        callOverlay = document.getElementById('call-overlay');
        callStatusText = document.getElementById('call-status-text');
        callTimerEl = document.getElementById('call-timer');
        remoteStreamsEl = document.getElementById('remote-streams');
        localVideoEl = document.getElementById('local-video');
        btnToggleMute = document.getElementById('btn-toggle-mute');
        btnToggleCamera = document.getElementById('btn-toggle-camera');
        btnEndCall = document.getElementById('btn-end-call');
        incomingCallBanner = document.getElementById('incoming-call');
        incomingCallerName = document.getElementById('incoming-caller-name');
        incomingCallType = document.getElementById('incoming-call-type');
        btnAcceptCall = document.getElementById('btn-accept-call');
        btnRejectCall = document.getElementById('btn-reject-call');

        btnScreenShare = document.getElementById('btn-screen-share');

        if (btnToggleMute) btnToggleMute.onclick = toggleMute;
        if (btnToggleCamera) btnToggleCamera.onclick = toggleCamera;
        if (btnEndCall) btnEndCall.onclick = endCall;
        if (btnAcceptCall) btnAcceptCall.onclick = acceptCall;
        if (btnRejectCall) btnRejectCall.onclick = rejectCall;
        if (btnScreenShare) btnScreenShare.onclick = toggleScreenShare;

        initVoiceQualityPanel();
    }

    function showCallOverlay(statusText, showCamera) {
        initDom();
        if (!callOverlay) return;
        callOverlay.style.display = '';
        updateCallStatus(statusText);
        if (callTimerEl) callTimerEl.textContent = '00:00';
        if (remoteStreamsEl) remoteStreamsEl.innerHTML = '';
        if (btnToggleCamera) btnToggleCamera.style.display = showCamera ? '' : 'none';
        // Hide local video preview for voice-only calls
        if (localVideoEl) localVideoEl.style.display = showCamera ? '' : 'none';
    }

    function hideCallOverlay() {
        if (callOverlay) callOverlay.style.display = 'none';
        if (localVideoEl) localVideoEl.srcObject = null;
        if (remoteStreamsEl) remoteStreamsEl.innerHTML = '';
    }

    function updateCallStatus(text) {
        if (callStatusText) callStatusText.textContent = text;
    }

    function showIncomingBanner(callerName, type) {
        initDom();
        if (!incomingCallBanner) return;
        if (incomingCallerName) incomingCallerName.textContent = callerName;
        if (incomingCallType) incomingCallType.textContent = (type === 'video' ? 'Video' : 'Voice') + ' Call';
        incomingCallBanner.style.display = '';
    }

    function hideIncomingBanner() {
        if (incomingCallBanner) incomingCallBanner.style.display = 'none';
    }

    function attachLocalStream() {
        if (localVideoEl && localStream) {
            localVideoEl.srcObject = localStream;
        }
    }

    // After devices are granted, push real tracks into any PCs that were created
    // before getUserMedia resolved (e.g. a peer joined while the permission prompt
    // was still open, so the PC was built with a black-canvas placeholder + null audio).
    function pushLocalStreamToPeerConnections() {
        if (!localStream) return;
        var audioTrack = localStream.getAudioTracks()[0] || null;
        var videoTrack = localStream.getVideoTracks()[0] || null;
        if (!audioTrack && !videoTrack) return;
        var peerIds = Object.keys(peerConnections);
        if (!peerIds.length) return;
        peerIds.forEach(function (userId) {
            var pc = peerConnections[userId];
            pc.getSenders().forEach(function (sender) {
                // Null track = placeholder audio sender (addTransceiver without a track)
                if (audioTrack && sender.track === null) {
                    sender.replaceTrack(audioTrack).catch(function (e) {
                        console.warn('[WebRTC] pushLocalStream audio replaceTrack failed for', userId, ':', e);
                    });
                }
                // Black canvas = placeholder video sender
                if (videoTrack && sender.track === blackVideoTrack) {
                    sender.replaceTrack(videoTrack).catch(function (e) {
                        console.warn('[WebRTC] pushLocalStream video replaceTrack failed for', userId, ':', e);
                    });
                }
            });
        });
        //console.log('[WebRTC] Pushed local stream tracks to', peerIds.length, 'existing peer connection(s)');
    }

    function addRemoteStreamElement(userId) {
        if (!remoteStreamsEl) return;
        var name = participantNames[userId] || userId.substring(0, 8);
        var container = document.createElement('div');
        container.className = 'remote-stream-item';
        container.id = 'remote-stream-' + userId;

        var mediaHtml;
        if (currentCallType === 'video') {
            mediaHtml = '<video id="remote-video-' + userId + '" autoplay muted playsinline></video>';
        } else {
            mediaHtml = '<audio id="remote-video-' + userId + '" autoplay muted></audio>' +
                '<div class="remote-stream-avatar"><span>' + escapeHtml(userId.substring(0, 2).toUpperCase()) + '</span></div>';
        }

        var kickHtml = isCallInitiator
            ? '<button class="remote-kick-btn" title="Remove participant">Kick</button>'
            : '';

        var overlayHtml = '<div class="remote-stream-overlay">' +
            '<span class="remote-stream-name">' + escapeHtml(name) + '</span>' +
            '<input type="range" class="remote-vol-slider" min="0" max="100" value="100" title="Volume">' +
            kickHtml +
            '</div>';

        container.innerHTML = mediaHtml + overlayHtml;
        remoteStreamsEl.appendChild(container);

        // Wire per-user volume slider
        var volSlider = container.querySelector('.remote-vol-slider');
        if (volSlider) {
            var vol = remoteVolumes[userId] !== undefined ? remoteVolumes[userId] : receiverVolume;
            volSlider.value = Math.round(vol * 100);
            volSlider.oninput = function () {
                setRemoteVolume(userId, parseInt(volSlider.value, 10) / 100);
            };
        }

        // Wire kick button
        var kickBtn = container.querySelector('.remote-kick-btn');
        if (kickBtn) {
            kickBtn.onclick = function () { kickParticipant(userId); };
        }

        // Apply initial volume
        var mediaEl = document.getElementById('remote-video-' + userId);
        var initVol = remoteVolumes[userId] !== undefined ? remoteVolumes[userId] : receiverVolume;
        if (mediaEl) mediaEl.volume = initVol;
        updateRemoteGrid();
    }

    function removeRemoteStreamElement(userId) {
        var el = document.getElementById('remote-stream-' + userId);
        if (el) el.remove();
        updateRemoteGrid();
    }

    function updateRemoteGrid() {
        if (!remoteStreamsEl) return;
        var count = remoteStreamsEl.children.length;
        if (count <= 1) {
            remoteStreamsEl.style.gridTemplateColumns = '1fr';
        } else if (count <= 4) {
            remoteStreamsEl.style.gridTemplateColumns = 'repeat(2, 1fr)';
        } else {
            remoteStreamsEl.style.gridTemplateColumns = 'repeat(3, 1fr)';
        }
    }

    function startCallTimer() {
        if (callTimerInterval) clearInterval(callTimerInterval);
        callTimerInterval = setInterval(function () {
            if (!callStartTime || !callTimerEl) return;
            var elapsed = Math.floor((Date.now() - callStartTime) / 1000);
            var m = Math.floor(elapsed / 60);
            var s = elapsed % 60;
            callTimerEl.textContent = (m < 10 ? '0' : '') + m + ':' + (s < 10 ? '0' : '') + s;
        }, 1000);
    }

    // Global call signal proxy (main tab only) 
    // global-call.js dispatches 'magnolia:send-signal' to send signals via this WS
    if (!isCallTab) {
        window.addEventListener('magnolia:send-signal', function (e) {
            sendSignal(e.detail);
        });
    }

    // Public API 

    return {
        connectWs: connectWs,
        disconnectWs: disconnectWs,
        startCall: startCall,
        joinCall: joinCall,
        acceptCall: acceptCall,
        rejectCall: rejectCall,
        endCall: endCall,
        toggleMute: toggleMute,
        toggleCamera: toggleCamera,
        getCallState: function () { return callState; },
        getCurrentCallId: function () { return currentCallId; },
        isCallTab: function () { return isCallTab; },
        initCallTab: initCallTab
    };
})();
