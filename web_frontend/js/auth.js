// Auth UI — login, register (open / invite-only / application mode), and helpers
var auth = (function () {

    // Called on init: if the hash contains #register/TOKEN, open register with token pre-filled
    function checkRegisterHash() {
        var hash = window.location.hash.replace('#', '');
        if (hash.indexOf('register/') === 0) {
            var token = hash.substring('register/'.length);
            window.location.hash = '';
            renderRegister(token);
            return true;
        }
        return false;
    }

    // Called on init: if the hash contains #reset-password/TOKEN, show set-password form
    function checkResetHash() {
        var hash = window.location.hash.replace('#', '');
        if (hash.indexOf('reset-password/') === 0) {
            var token = hash.substring('reset-password/'.length).trim();
            window.location.hash = '';
            renderSetPassword(token);
            return true;
        }
        return false;
    }

    function renderSetPassword(token) {
        var form = document.getElementById('auth-form');
        form.innerHTML =
            '<p style="margin:0 0 1rem;color:var(--t-secondary);font-size:0.875rem">Set a password for your new account.</p>' +
            '<input type="hidden" id="sp-token" value="' + escHtml(token) + '">' +
            '<div class="form-group"><label>New Password</label>' +
            '<input id="sp-password" class="form-input" type="password" placeholder="Min 12 characters, 1 number, 1 symbol (?!$...)" autocomplete="new-password"></div>' +
            '<div class="form-group"><label>Confirm Password</label>' +
            '<input id="sp-confirm" class="form-input" type="password" placeholder="Repeat password" autocomplete="new-password"></div>' +
            '<button id="sp-btn" class="btn btn-primary" style="width:100%">Set Password</button>';

        document.getElementById('auth-toggle').innerHTML =
            'Already have a password? <a href="#" id="goto-login-sp">Log in</a>';

        document.getElementById('sp-btn').onclick = doSetPassword;
        document.getElementById('sp-confirm').onkeydown = function (e) {
            if (e.key === 'Enter') doSetPassword();
        };
        document.getElementById('goto-login-sp').onclick = function (e) {
            e.preventDefault();
            hideError();
            renderLogin();
        };
    }

    async function doSetPassword() {
        var token = document.getElementById('sp-token').value;
        var password = document.getElementById('sp-password').value;
        var confirm = document.getElementById('sp-confirm').value;
        if (!password || !confirm) { showError('Please fill in all fields'); return; }
        if (password !== confirm) { showError('Passwords do not match'); return; }
        if (password.length < 8) { showError('Password must be at least 8 characters'); return; }
        disableBtn('sp-btn');
        try {
            await api.post('/api/auth/reset-password', {
                token: token,
                password: password,
                password_confirm: confirm
            });
            hideError();
            showInfo('Password set! You can now log in.');
            setTimeout(function () { renderLogin(); }, 1500);
        } catch (e) {
            showError(e.message);
        }
        enableBtn('sp-btn', 'Set Password');
    }

    function renderSetup() {
        var form = document.getElementById('auth-form');
        form.innerHTML =
            '<p style="margin:0 0 1rem;color:var(--t-secondary);font-size:0.875rem">' +
            'Welcome! Create the first admin account to get started.</p>' +
            '<div class="form-group"><label>Username (case-sensitive)</label>' +
            '<input id="setup-username" class="form-input" type="text" placeholder="admin" autocomplete="username"></div>' +
            '<div class="form-group"><label>Email</label>' +
            '<input id="setup-email" class="form-input" type="email" placeholder="admin@example.com" autocomplete="email"></div>' +
            '<div class="form-group"><label>Password</label>' +
            '<input id="setup-password" class="form-input" type="password" placeholder="Min 12 characters, 1 number, 1 symbol (?!$...)" autocomplete="new-password"></div>' +
            '<div class="form-group"><label>Confirm Password</label>' +
            '<input id="setup-confirm" class="form-input" type="password" placeholder="Repeat password" autocomplete="new-password"></div>' +
            '<button id="setup-btn" class="btn btn-primary" style="width:100%">Create Admin Account</button>';

        document.getElementById('auth-toggle').innerHTML = '';

        document.getElementById('setup-btn').onclick = doSetup;
        document.getElementById('setup-confirm').onkeydown = function (e) {
            if (e.key === 'Enter') doSetup();
        };
    }

    async function doSetup() {
        var username = document.getElementById('setup-username').value.trim();
        var email = document.getElementById('setup-email').value.trim();
        var password = document.getElementById('setup-password').value;
        var confirm = document.getElementById('setup-confirm').value;
        if (!username || !email || !password || !confirm) { showError('Please fill in all fields'); return; }
        if (password !== confirm) { showError('Passwords do not match'); return; }
        if (password.length < 8) { showError('Password must be at least 8 characters'); return; }
        disableBtn('setup-btn');
        try {
            await api.post('/api/setup', { username: username, email: email, password: password });
            hideError();
            showInfo('Admin account created! You can now log in.');
            setTimeout(function () { renderLogin(); }, 1500);
        } catch (e) {
            showError(e.message);
        }
        enableBtn('setup-btn', 'Create Admin Account');
    }

    function renderLogin() {
        var form = document.getElementById('auth-form');
        form.innerHTML =
            '<div class="form-group"><label>Username or Email (case-sensitive)</label>' +
            '<input id="login-identifier" class="form-input" type="text" placeholder="username or you@example.com" autocomplete="username"></div>' +
            '<div class="form-group"><label>Password</label>' +
            '<input id="login-password" class="form-input" type="password" placeholder="Your password" autocomplete="current-password"></div>' +
            '<button id="login-btn" class="btn btn-primary" style="width:100%">Log in</button>';

        document.getElementById('auth-toggle').innerHTML =
            'Don\'t have an account? <a href="#" id="goto-register">Register</a>';

        document.getElementById('login-btn').onclick = doLogin;
        document.getElementById('login-password').onkeydown = function (e) {
            if (e.key === 'Enter') doLogin();
        };
        document.getElementById('goto-register').onclick = function (e) {
            e.preventDefault();
            hideError();
            loadAndRenderRegister();
        };
    }

    // Fetch registration mode then choose which form to show
    function loadAndRenderRegister() {
        api.get('/api/auth/config').then(function (cfg) {
            renderRegister(null, cfg && cfg.registration_mode);
        }).catch(function () {
            renderRegister(null, 'open');
        });
    }

    // Render the appropriate register/apply form based on mode.
    // prefillToken: invite token from URL hash (invite_only mode)
    // mode: 'open' | 'invite_only' | 'application' — if omitted, fetched from server
    function renderRegister(prefillToken, mode) {
        if (mode === undefined) {
            // Fetch mode then re-call
            api.get('/api/auth/config').then(function (cfg) {
                renderRegister(prefillToken, cfg ? cfg.registration_mode : 'open');
            }).catch(function () {
                renderRegister(prefillToken, 'open');
            });
            return;
        }

        hideError();

        // A prefillToken means the user arrived via an invite link — always show
        // the registration form with the token pre-filled, even if the site's
        // default mode is 'application'.
        if (mode === 'application' && !prefillToken) {
            renderApplicationForm();
            return;
        }

        // open, invite_only, or invite-link override — show standard register form
        var tokenField = (mode === 'invite_only' || prefillToken)
            ? '<div class="form-group"><label>Invite Code</label>' +
            '<input id="reg-token" class="form-input" type="text" placeholder="Paste your invite code" value="' +
            escHtml(prefillToken || '') + '" autocomplete="off"></div>'
            : '';

        var form = document.getElementById('auth-form');
        form.innerHTML =
            tokenField +
            '<div class="form-group"><label>Username (case-sensitive)</label>' +
            '<input id="reg-username" class="form-input" type="text" placeholder="your_username" autocomplete="username"></div>' +
            '<div class="form-group"><label>Email <span style="color:var(--t-muted)">(optional)</span></label>' +
            '<input id="reg-email" class="form-input" type="email" placeholder="you@example.com" autocomplete="email"></div>' +
            '<div class="form-group"><label>Password</label>' +
            '<input id="reg-password" class="form-input" type="password" placeholder="Min 12 characters, 1 number, 1 symbol (?!$...)" autocomplete="new-password"></div>' +
            '<div class="form-group"><label>Confirm Password</label>' +
            '<input id="reg-confirm" class="form-input" type="password" placeholder="Repeat password" autocomplete="new-password"></div>' +
            '<button id="reg-btn" class="btn btn-primary" style="width:100%">Register</button>';

        document.getElementById('auth-toggle').innerHTML =
            'Already have an account? <a href="#" id="goto-login">Log in</a>';

        // If a token field is shown (invite link or invite_only mode), treat as invite_only
        // so doRegister will read and send the token.
        var effectiveMode = (mode === 'invite_only' || prefillToken) ? 'invite_only' : mode;
        document.getElementById('reg-btn').onclick = function () { doRegister(effectiveMode); };
        document.getElementById('reg-confirm').onkeydown = function (e) {
            if (e.key === 'Enter') doRegister(effectiveMode);
        };
        document.getElementById('goto-login').onclick = function (e) {
            e.preventDefault();
            hideError();
            renderLogin();
        };
    }

    function renderApplicationForm() {
        var form = document.getElementById('auth-form');
        form.innerHTML =
            '<p style="margin:0 0 1rem;color:var(--t-secondary);font-size:0.875rem">' +
            'Registration requires admin approval. Fill in the form and your application will be reviewed.</p>' +
            '<div class="form-group"><label>Email</label>' +
            '<input id="app-email" class="form-input" type="email" placeholder="you@example.com" autocomplete="username"></div>' +
            '<div class="form-group"><label>Display Name <span style="color:var(--t-muted)">(optional)</span></label>' +
            '<input id="app-name" class="form-input" type="text" placeholder="Your name" autocomplete="name"></div>' +
            '<div class="form-group"><label>Message <span style="color:var(--t-muted)">(optional)</span></label>' +
            '<textarea id="app-message" class="form-input" rows="3" placeholder="Why do you want to join?" style="resize:vertical"></textarea></div>' +
            '<button id="app-btn" class="btn btn-primary" style="width:100%">Submit Application</button>';

        document.getElementById('auth-toggle').innerHTML =
            'Already have an account? <a href="#" id="goto-login">Log in</a>';

        document.getElementById('app-btn').onclick = doSubmitApplication;
        document.getElementById('goto-login').onclick = function (e) {
            e.preventDefault();
            hideError();
            renderLogin();
        };
    }

    async function doLogin() {
        var identifier = document.getElementById('login-identifier').value.trim();
        var password = document.getElementById('login-password').value;
        if (!identifier || !password) { showError('Please fill in all fields'); return; }
        disableBtn('login-btn');
        try {
            await api.post('/api/auth/login', { identifier: identifier, password: password });
            hideError();
            app.onAuthenticated();
        } catch (e) {
            showError(e.message);
        }
        enableBtn('login-btn', 'Log in');
    }

    async function doRegister(mode) {
        var username = document.getElementById('reg-username').value.trim();
        var email = document.getElementById('reg-email').value.trim();
        var password = document.getElementById('reg-password').value;
        var confirm = document.getElementById('reg-confirm').value;
        if (!username) { showError('Username is required'); return; }
        if (!password || !confirm) { showError('Please enter a password'); return; }
        if (password !== confirm) { showError('Passwords do not match'); return; }
        if (password.length < 8) { showError('Password must be at least 8 characters'); return; }

        var body = { username: username, password: password, password_confirm: confirm };
        if (email) body.email = email;
        if (mode === 'invite_only') {
            var tokenEl = document.getElementById('reg-token');
            var token = tokenEl ? tokenEl.value.trim() : '';
            if (!token) { showError('An invite code is required'); return; }
            body.invite_token = token;
        }

        disableBtn('reg-btn');
        try {
            await api.post('/api/auth/register', body);
            hideError();
            showInfo('Account created! You can now log in.');
            setTimeout(function () { renderLogin(); }, 1500);
        } catch (e) {
            showError(e.message);
        }
        enableBtn('reg-btn', 'Register');
    }

    async function doSubmitApplication() {
        var email = document.getElementById('app-email').value.trim();
        var name = document.getElementById('app-name').value.trim();
        var message = document.getElementById('app-message').value.trim();
        if (!email) { showError('Email is required'); return; }
        disableBtn('app-btn');
        try {
            await api.post('/api/auth/apply', {
                email: email,
                display_name: name || null,
                message: message || null
            });
            hideError();
            showInfo('Application submitted! You will be notified when it\'s reviewed.');
            setTimeout(function () { renderLogin(); }, 2500);
        } catch (e) {
            showError(e.message);
        }
        enableBtn('app-btn', 'Submit Application');
    }

    function showError(msg) {
        var el = document.getElementById('auth-error');
        el.textContent = msg;
        el.className = 'error-box';
        el.style.display = '';
    }

    function showInfo(msg) {
        var el = document.getElementById('auth-error');
        el.textContent = msg;
        el.className = 'info-box';
        el.style.display = '';
    }

    function hideError() {
        document.getElementById('auth-error').style.display = 'none';
    }

    function disableBtn(id) {
        var btn = document.getElementById(id);
        if (btn) { btn.disabled = true; btn.textContent = 'Please wait...'; }
    }

    function enableBtn(id, label) {
        var btn = document.getElementById(id);
        if (btn) { btn.disabled = false; btn.textContent = label; }
    }

    function escHtml(s) {
        return String(s)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    return {
        renderLogin: renderLogin,
        renderSetup: renderSetup,
        renderRegister: loadAndRenderRegister,
        checkRegisterHash: checkRegisterHash,
        checkResetHash: checkResetHash
    };
})();
