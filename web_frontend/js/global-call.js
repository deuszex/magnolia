// Global always-on voice call embedded in the main tab
var globalCall = (function () {
    var state = {
        inCall: false,
        muted: false,
        participants: [], // [{user_id, display_name, joined_at}]
        peerConnections: {}, // userId -> RTCPeerConnection
        localStream: null,
    };

    var iceServers = [];

    // Persistent settings keys
    var SK = {
        mic: 'magnolia_gc_mic',
        speaker: 'magnolia_gc_speaker',
        volume: 'magnolia_gc_volume',
        echoCancel: 'magnolia_gc_echo',
        noiseSuppress: 'magnolia_gc_noise',
        autoGain: 'magnolia_gc_gain',
    };

    function getPref(key, fallback) {
        var v = localStorage.getItem(key);
        return v !== null ? v : fallback;
    }
    function setPref(key, val) { localStorage.setItem(key, String(val)); }

    // DOM helpers 

    function getPanel() { return document.getElementById('global-call-panel'); }
    function getJoinBtn() { return document.getElementById('btn-global-call-join'); }
    function getLeaveBtn() { return document.getElementById('btn-global-call-leave'); }
    function getMuteBtn() { return document.getElementById('btn-global-call-mute'); }
    function getParticipantsEl() { return document.getElementById('global-call-participants'); }
    function getCountEl() { return document.getElementById('global-call-count'); }

    // Initialization 

    function init() {
        api.get('/api/calls/ice-config').then(function (cfg) {
            if (cfg && cfg.ice_servers && cfg.ice_servers.length) {
                iceServers = cfg.ice_servers.map(function (s) {
                    return { urls: s.urls, username: s.username, credential: s.credential };
                });
            }
        }).catch(function () { });

        var joinBtn = getJoinBtn();
        var leaveBtn = getLeaveBtn();
        var muteBtn = getMuteBtn();
        var settingsBtn = document.getElementById('btn-global-call-settings');
        if (joinBtn) joinBtn.onclick = join;
        if (leaveBtn) leaveBtn.onclick = leave;
        if (muteBtn) muteBtn.onclick = toggleMute;
        if (settingsBtn) settingsBtn.onclick = toggleDeviceSettings;

        // Listen for WS signals forwarded by calling.js via custom events
        window.addEventListener('magnolia:signal', function (e) {
            handleSignal(e.detail);
        });

        // Fetch initial state
        fetchParticipants();
    }

    // REST calls 

    function fetchParticipants() {
        api.get('/api/global-call').then(function (data) {
            if (data && data.participants) {
                state.participants = data.participants;
                renderPanel();
            }
        }).catch(function () { });
    }

    // Join / Leave 

    function join() {
        if (state.inCall) return;

        var audioConstraints = buildAudioConstraints();

        // Try to get a mic; fall back to receive-only if no device / permission denied.
        // If the stored device is gone, clear it and retry with defaults.
        var mediaPromise = (navigator.mediaDevices && navigator.mediaDevices.getUserMedia)
            ? navigator.mediaDevices.getUserMedia({ audio: audioConstraints, video: false })
                .catch(function (err) {
                    if (err.name === 'OverconstrainedError' || err.name === 'NotFoundError') {
                        setPref(SK.mic, '');
                        return navigator.mediaDevices.getUserMedia({ audio: buildAudioConstraints(), video: false })
                            .catch(function (err2) {
                                console.warn('Global call: no mic available, joining receive-only:', err2.name);
                                return null;
                            });
                    }
                    console.warn('Global call: no mic available, joining receive-only:', err.name);
                    return null;
                })
            : Promise.resolve(null);

        mediaPromise.then(function (stream) {
            state.localStream = stream; // may be null (receive-only)
            state.inCall = true;
            state.muted = false;
            applyVolume();
            renderPanel();

            api.post('/api/global-call/join').then(function (data) {
                if (data && data.participants) {
                    state.participants = data.participants;
                    renderPanel();
                }
                // Offer to every participant already in the call (except self)
                var myId = app.state.currentUser && app.state.currentUser.user_id;
                state.participants.forEach(function (p) {
                    if (p.user_id !== myId) {
                        createAndSendOffer(p.user_id);
                    }
                });
            }).catch(function (err) {
                console.error('Global call join failed:', err);
                stopLocalStream();
                state.inCall = false;
                renderPanel();
            });
        });
    }

    function leave() {
        if (!state.inCall) return;

        // Close all peer connections
        Object.keys(state.peerConnections).forEach(function (uid) {
            closePeer(uid);
        });
        stopLocalStream();
        state.inCall = false;
        state.muted = false;

        api.post('/api/global-call/leave').then(function (data) {
            if (data && data.participants) {
                state.participants = data.participants;
            }
            renderPanel();
        }).catch(function () { renderPanel(); });
    }

    function toggleMute() {
        if (!state.localStream) return;
        state.muted = !state.muted;
        state.localStream.getAudioTracks().forEach(function (t) {
            t.enabled = !state.muted;
        });
        renderPanel();
    }

    // WebRTC helpers 

    function getOrCreatePeerConnection(targetUserId) {
        if (state.peerConnections[targetUserId]) {
            return state.peerConnections[targetUserId];
        }
        var pc = new RTCPeerConnection({ iceServers: iceServers });
        state.peerConnections[targetUserId] = pc;

        // Add local audio tracks
        if (state.localStream) {
            state.localStream.getTracks().forEach(function (t) {
                pc.addTrack(t, state.localStream);
            });
        }

        // On receiving remote audio — attach to a hidden <audio> element
        pc.ontrack = function (e) {
            var audioId = 'global-call-audio-' + targetUserId;
            var audio = document.getElementById(audioId);
            if (!audio) {
                audio = document.createElement('audio');
                audio.id = audioId;
                audio.autoplay = true;
                document.body.appendChild(audio);
            }
            audio.srcObject = e.streams[0];
            applyVolume(audio);
            applyOutputDevice(audio);
        };

        pc.onicecandidate = function (e) {
            if (e.candidate) {
                sendSignal({
                    type: 'global_call_ice',
                    target_user_id: targetUserId,
                    candidate: e.candidate.toJSON(),
                });
            }
        };

        pc.onconnectionstatechange = function () {
            if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
                closePeer(targetUserId);
            }
        };

        return pc;
    }

    function closePeer(userId) {
        var pc = state.peerConnections[userId];
        if (pc) {
            pc.close();
            delete state.peerConnections[userId];
        }
        var audio = document.getElementById('global-call-audio-' + userId);
        if (audio) {
            audio.srcObject = null;
            audio.remove();
        }
    }

    function stopLocalStream() {
        if (state.localStream) {
            state.localStream.getTracks().forEach(function (t) { t.stop(); });
            state.localStream = null;
        }
    }

    function createAndSendOffer(targetUserId) {
        var pc = getOrCreatePeerConnection(targetUserId);
        pc.createOffer().then(function (offer) {
            return pc.setLocalDescription(offer).then(function () {
                sendSignal({
                    type: 'global_call_offer',
                    target_user_id: targetUserId,
                    sdp: offer.sdp,
                });
            });
        }).catch(function (err) {
            console.error('Global call createOffer failed:', err);
        });
    }

    // Incoming WS signal handler 

    function handleSignal(data) {
        if (!data || !data.type) return;

        switch (data.type) {
            case 'global_call_update':
                state.participants = data.participants || [];
                renderPanel();
                renderSidebarIndicators();
                if (state.inCall) {
                    // Close PCs for users who have left — do NOT send new offers here.
                    // New joiners are responsible for sending offers to existing participants;
                    // existing participants only answer. Sending from both sides causes glare.
                    Object.keys(state.peerConnections).forEach(function (uid) {
                        var still = state.participants.some(function (p) { return p.user_id === uid; });
                        if (!still) closePeer(uid);
                    });
                }
                break;

            case 'global_call_offer':
                if (!state.inCall) break;
                handleIncomingOffer(data.from_user_id, data.sdp);
                break;

            case 'global_call_answer':
                if (!state.inCall) break;
                handleIncomingAnswer(data.from_user_id, data.sdp);
                break;

            case 'global_call_ice':
                if (!state.inCall) break;
                handleIncomingIce(data.from_user_id, data.candidate);
                break;
        }
    }

    function handleIncomingOffer(fromUserId, sdp) {
        var pc = getOrCreatePeerConnection(fromUserId);
        pc.setRemoteDescription({ type: 'offer', sdp: sdp }).then(function () {
            return pc.createAnswer();
        }).then(function (answer) {
            return pc.setLocalDescription(answer).then(function () {
                sendSignal({
                    type: 'global_call_answer',
                    target_user_id: fromUserId,
                    sdp: answer.sdp,
                });
            });
        }).catch(function (err) {
            console.error('Global call handleIncomingOffer failed:', err);
        });
    }

    function handleIncomingAnswer(fromUserId, sdp) {
        var pc = state.peerConnections[fromUserId];
        if (!pc) return;
        pc.setRemoteDescription({ type: 'answer', sdp: sdp }).catch(function (err) {
            console.error('Global call setRemoteDescription (answer) failed:', err);
        });
    }

    function handleIncomingIce(fromUserId, candidate) {
        var pc = state.peerConnections[fromUserId];
        if (!pc) return;
        pc.addIceCandidate(new RTCIceCandidate(candidate)).catch(function (err) {
            console.warn('Global call addIceCandidate failed:', err);
        });
    }

    // Send signal via WS (proxied through calling.js) 

    function sendSignal(payload) {
        window.dispatchEvent(new CustomEvent('magnolia:send-signal', { detail: payload }));
    }

    // Audio settings helpers 

    // Build MediaTrackConstraints from stored preferences
    function buildAudioConstraints() {
        var mic = getPref(SK.mic, '');
        var echo = getPref(SK.echoCancel, 'true') === 'true';
        var noise = getPref(SK.noiseSuppress, 'true') === 'true';
        var gain = getPref(SK.autoGain, 'true') === 'true';
        var c = {
            echoCancellation: echo,
            noiseSuppression: noise,
            autoGainControl: gain,
        };
        if (mic) c.deviceId = { exact: mic };
        return c;
    }

    // Set volume on one or all remote audio elements (val is 0-1, defaults to stored pref)
    function applyVolume(audioEl) {
        var vol = parseFloat(getPref(SK.volume, '100')) / 100;
        if (audioEl) {
            audioEl.volume = vol;
        } else {
            document.querySelectorAll('[id^="global-call-audio-"]').forEach(function (el) {
                el.volume = vol;
            });
        }
    }

    // Route audio output to the saved speaker device (Chromium only; no-op elsewhere)
    function applyOutputDevice(audioEl) {
        var spk = getPref(SK.speaker, '');
        if (!spk || typeof audioEl.setSinkId !== 'function') return;
        audioEl.setSinkId(spk).catch(function (e) {
            console.warn('Global call: setSinkId failed:', e);
        });
    }

    // Replace the mic track in every peer connection without renegotiating
    function replaceMicTrack(newStream) {
        var newTrack = newStream ? newStream.getAudioTracks()[0] : null;
        Object.values(state.peerConnections).forEach(function (pc) {
            pc.getSenders().forEach(function (sender) {
                if (sender.track && sender.track.kind === 'audio') {
                    sender.replaceTrack(newTrack || null).catch(function (e) {
                        console.warn('Global call: replaceTrack failed:', e);
                    });
                }
            });
        });
    }

    // Device settings panel 

    function toggleDeviceSettings() {
        var panel = document.getElementById('global-call-settings-panel');
        if (!panel) return;
        var opening = panel.style.display === 'none';
        panel.style.display = opening ? '' : 'none';
        if (opening) populateSettingsPanel();
    }

    function populateSettingsPanel() {
        if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) return;

        // Show speaker section only when setSinkId is available (Chromium)
        var speakerRow = document.getElementById('gc-speaker-row');
        if (speakerRow) {
            speakerRow.style.display = ('setSinkId' in HTMLAudioElement.prototype) ? '' : 'none';
        }

        // Restore volume slider
        var volSlider = document.getElementById('gc-volume');
        var volPct = document.getElementById('gc-volume-pct');
        if (volSlider) {
            volSlider.value = getPref(SK.volume, '100');
            if (volPct) volPct.textContent = volSlider.value + '%';
            volSlider.oninput = function () {
                setPref(SK.volume, volSlider.value);
                if (volPct) volPct.textContent = volSlider.value + '%';
                applyVolume();
            };
        }

        // Restore audio-processing checkboxes
        function wireCheck(id, key) {
            var el = document.getElementById(id);
            if (!el) return;
            el.checked = getPref(key, 'true') === 'true';
            el.onchange = function () {
                setPref(key, el.checked ? 'true' : 'false');
                var note = document.getElementById('gc-processing-note');
                if (note) note.style.display = state.inCall ? '' : 'none';
            };
        }
        wireCheck('gc-echo-cancel', SK.echoCancel);
        wireCheck('gc-noise-suppress', SK.noiseSuppress);
        wireCheck('gc-auto-gain', SK.autoGain);

        // Request permission then enumerate so we get real device labels
        var doEnum = function () {
            navigator.mediaDevices.enumerateDevices().then(function (devices) {
                fillDeviceSelect(
                    'gc-mic-select', 'audioinput', SK.mic,
                    devices,
                    function (deviceId) {
                        setPref(SK.mic, deviceId);
                        if (!state.inCall) return;
                        // Hot-swap mic while in-call via replaceTrack
                        var c = buildAudioConstraints();
                        navigator.mediaDevices.getUserMedia({ audio: c, video: false })
                            .then(function (newStream) {
                                replaceMicTrack(newStream);
                                stopLocalStream();
                                state.localStream = newStream;
                                state.muted = false;
                                renderPanel();
                            })
                            .catch(function (e) { console.warn('Mic swap failed:', e); });
                    }
                );
                fillDeviceSelect(
                    'gc-speaker-select', 'audiooutput', SK.speaker,
                    devices,
                    function (deviceId) {
                        setPref(SK.speaker, deviceId);
                        // Apply immediately to all live audio elements
                        document.querySelectorAll('[id^="global-call-audio-"]').forEach(function (el) {
                            if (typeof el.setSinkId === 'function') {
                                el.setSinkId(deviceId).catch(function (e) {
                                    console.warn('setSinkId failed:', e);
                                });
                            }
                        });
                    }
                );
            });
        };

        // Brief getUserMedia request to unlock device labels, then stop immediately
        navigator.mediaDevices.getUserMedia({ audio: true, video: false })
            .then(function (s) { s.getTracks().forEach(function (t) { t.stop(); }); doEnum(); })
            .catch(function () { doEnum(); });
    }

    function fillDeviceSelect(selectId, kind, storageKey, devices, onChange) {
        var select = document.getElementById(selectId);
        if (!select) return;
        var list = devices.filter(function (d) { return d.kind === kind; });
        select.innerHTML = '';
        if (list.length === 0) {
            select.innerHTML = '<option value="">None found</option>';
            return;
        }
        var savedId = getPref(storageKey, '');
        list.forEach(function (d, i) {
            var opt = document.createElement('option');
            opt.value = d.deviceId;
            opt.textContent = d.label || (kind === 'audioinput' ? 'Microphone ' : 'Speaker ') + (i + 1);
            if (d.deviceId === savedId) opt.selected = true;
            select.appendChild(opt);
        });
        select.onchange = function () { onChange(select.value); };
    }

    // Render 

    function renderPanel() {
        var panel = getPanel();
        if (!panel) return;

        var count = state.participants.length;
        var countEl = getCountEl();
        if (countEl) countEl.textContent = count > 0 ? '(' + count + ')' : '';

        var participantsEl = getParticipantsEl();
        if (participantsEl) {
            if (count === 0) {
                participantsEl.textContent = 'No one in the call';
            } else {
                participantsEl.innerHTML = '';
                state.participants.forEach(function (p) {
                    var span = document.createElement('span');
                    span.className = 'global-call-participant-name';
                    span.textContent = p.display_name || p.user_id.substring(0, 8);
                    participantsEl.appendChild(span);
                });
            }
        }

        var joinBtn = getJoinBtn();
        var leaveBtn = getLeaveBtn();
        var muteBtn = getMuteBtn();
        if (joinBtn) joinBtn.style.display = state.inCall ? 'none' : '';
        if (leaveBtn) leaveBtn.style.display = state.inCall ? '' : 'none';
        if (muteBtn) {
            // Only show mute button when we actually have a local stream
            muteBtn.style.display = (state.inCall && state.localStream) ? '' : 'none';
            muteBtn.textContent = state.muted ? 'Unmute' : 'Mute';
            muteBtn.classList.toggle('btn-muted', state.muted);
        }
    }

    function renderSidebarIndicators() {
        // Update .user-item elements in the users sidebar
        var inCallIds = {};
        state.participants.forEach(function (p) { inCallIds[p.user_id] = true; });

        document.querySelectorAll('.user-item[data-user-id]').forEach(function (el) {
            var uid = el.getAttribute('data-user-id');
            var indicator = el.querySelector('.global-call-indicator');
            if (inCallIds[uid]) {
                if (!indicator) {
                    indicator = document.createElement('span');
                    indicator.className = 'global-call-indicator';
                    indicator.title = 'In global call';
                    el.appendChild(indicator);
                }
            } else {
                if (indicator) indicator.remove();
            }
        });
    }

    return {
        init: init,
        handleSignal: handleSignal,
        getParticipants: function () { return state.participants; },
        isInCall: function () { return state.inCall; },
    };
})();
