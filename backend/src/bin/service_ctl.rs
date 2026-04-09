//! Windows service control utility.
//!
//! This binary provides commands to install, uninstall, start, stop,
//! and query the status of the magnolia Windows service.
//!
//! Usage:
//! service_ctl install - Install the service
//! service_ctl uninstall - Uninstall the service
//! service_ctl start - Start the service
//! service_ctl stop - Stop the service
//! service_ctl status - Query service status

use std::env;
use std::error::Error;
use std::ffi::OsString;

#[cfg(windows)]
use windows_service::{
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};

#[cfg(windows)]
use magnolia_server::service::windows::{SERVICE_DESCRIPTION, SERVICE_DISPLAY_NAME, SERVICE_NAME};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = args[1].as_str();

    #[cfg(windows)]
    {
        let result = match command {
            "install" => install_service(),
            "uninstall" => uninstall_service(),
            "start" => start_service(),
            "stop" => stop_service(),
            "status" => query_status(),
            _ => {
                print_usage();
                std::process::exit(1);
            }
        };

        if let Err(e) = result {
            eprintln!("Error: {:?}", e);
            std::process::exit(1);
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!("This utility is only available on Windows.");
        eprintln!("On Linux, use systemctl to manage the service.");
        eprintln!("On macOS, use launchctl to manage the service.");
        std::process::exit(1);
    }
}

fn print_usage() {
    eprintln!("Magnolia Service Control Utility");
    eprintln!();
    eprintln!("Usage: service_ctl <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!(" install - Install the Windows service");
    eprintln!(" uninstall - Uninstall the Windows service");
    eprintln!(" start - Start the service");
    eprintln!(" stop - Stop the service");
    eprintln!(" status - Query service status");
}

#[cfg(windows)]
fn install_service() -> Result<(), Box<dyn Error>> {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    // Get the path to the main executable
    let service_binary_path = env::current_exe()?
        .parent()
        .ok_or("Failed to get executable directory")?
        .join("magnolia.exe");

    if !service_binary_path.exists() {
        return Err(format!(
            "Service binary not found at: {}",
            service_binary_path.display()
        )
        .into());
    }

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: service_binary_path,
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None, // LocalSystem account
        account_password: None,
    };

    let service = service_manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;

    // Set service description
    service.set_description(SERVICE_DESCRIPTION)?;

    println!("Service '{}' installed successfully.", SERVICE_NAME);
    println!("The service is configured to start automatically on system boot.");
    println!("Use 'service_ctl start' to start the service now.");

    Ok(())
}

#[cfg(windows)]
fn uninstall_service() -> Result<(), Box<dyn Error>> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let service = service_manager.open_service(SERVICE_NAME, service_access)?;

    // Try to stop the service first if it's running
    let status = service.query_status()?;
    if status.current_state != ServiceState::Stopped {
        println!("Stopping service...");
        service.stop()?;

        // Wait for the service to stop (hopefully)
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    // Delete the service
    service.delete()?;

    println!("Service '{}' uninstalled successfully.", SERVICE_NAME);

    Ok(())
}

#[cfg(windows)]
fn start_service() -> Result<(), Box<dyn Error>> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service = service_manager.open_service(SERVICE_NAME, ServiceAccess::START)?;

    service.start::<String>(&[])?;

    println!("Service '{}' started.", SERVICE_NAME);

    Ok(())
}

#[cfg(windows)]
fn stop_service() -> Result<(), Box<dyn Error>> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service = service_manager.open_service(SERVICE_NAME, ServiceAccess::STOP)?;

    service.stop()?;

    println!("Service '{}' stopped.", SERVICE_NAME);

    Ok(())
}

#[cfg(windows)]
fn query_status() -> Result<(), Box<dyn Error>> {
    let manager_access = ServiceManagerAccess::CONNECT;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service = service_manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)?;

    let status = service.query_status()?;

    let state_str = match status.current_state {
        ServiceState::Stopped => "Stopped",
        ServiceState::StartPending => "Start Pending",
        ServiceState::StopPending => "Stop Pending",
        ServiceState::Running => "Running",
        ServiceState::ContinuePending => "Continue Pending",
        ServiceState::PausePending => "Pause Pending",
        ServiceState::Paused => "Paused",
    };

    println!("Service: {}", SERVICE_NAME);
    println!("Status: {}", state_str);

    Ok(())
}
