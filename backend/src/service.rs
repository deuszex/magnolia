//! Windows service implementation for the magnolia server.
//!
//! This module provides Windows service functionality, allowing the server
//! to run as a background service that starts automatically on system boot.

#[cfg(windows)]
pub mod windows {
    use std::ffi::OsString;
    use std::sync::mpsc;
    use std::time::Duration;
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    /// Service name used for registration and management
    pub const SERVICE_NAME: &str = "MagnoliaServer";

    /// Display name shown in Windows Services manager
    pub const SERVICE_DISPLAY_NAME: &str = "Magnolia Server";

    /// Service description
    pub const SERVICE_DESCRIPTION: &str = "Magnolia Anti-Social Community Platform Server. Provides Web Interface (Browser) and API (Developer thingy)";

    // Define the Windows service entry point
    define_windows_service!(ffi_service_main, service_main);

    /// Main entry point when running as a Windows service.
    /// Called by the Windows Service Control Manager.
    pub fn run_as_service() -> Result<(), windows_service::Error> {
        // Register and start the service dispatcher
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
        Ok(())
    }

    /// Service main function - handles service lifecycle
    fn service_main(arguments: Vec<OsString>) {
        if let Err(e) = run_service(arguments) {
            // Log error (service may not have console access)
            tracing::error!("Service error: {:?}", e);
        }
    }

    /// Actual service implementation
    fn run_service(
        _arguments: Vec<OsString>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create a channel to receive stop events
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        // Define the service control handler
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    // Send shutdown signal
                    let _ = shutdown_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        // Register the service control handler
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        // Report that we're starting
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })?;

        // Build and start the tokio runtime
        let runtime = tokio::runtime::Runtime::new()?;

        // Start the server in a background task
        let server_handle = runtime.spawn(async {
            if let Err(e) = crate::run_server().await {
                tracing::error!("Server error: {:?}", e);
            }
        });

        // Report that we're running
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        // Wait for shutdown signal
        let _ = shutdown_rx.recv();

        // Report that we're stopping
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StopPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })?;

        // Abort the server task and shutdown runtime
        server_handle.abort();
        runtime.shutdown_timeout(Duration::from_secs(5));

        // Report that we've stopped
        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }

    /// Check if the process was started by the Service Control Manager.
    /// Returns true if running as a service, false if running as console application.
    pub fn is_running_as_service() -> bool {
        // Check if we have a console window attached
        // Services started by SCM typically don't have a console
        // GetConsoleWindow returns 0 (null HWND) if no console is attached
        let console = unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() };
        console.is_null()
    }
}

#[cfg(not(windows))]
pub mod windows {
    /// Stub for non-Windows platforms
    pub fn run_as_service() -> Result<(), std::io::Error> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Windows service is only available on Windows",
        ))
    }

    /// Always returns false on non-Windows platforms
    pub fn is_running_as_service() -> bool {
        false
    }

    pub const SERVICE_NAME: &str = "magnoliaServer";
    pub const SERVICE_DISPLAY_NAME: &str = "Magnolia Server";
    pub const SERVICE_DESCRIPTION: &str = "Magnolia Anti-Social Community Platform Server. Provides Web Interface (Browser) and API (Developer thingy)";
}
