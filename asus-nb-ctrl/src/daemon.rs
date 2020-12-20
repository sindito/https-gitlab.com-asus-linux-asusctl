use ctrl_gfx::ctrl_gfx::CtrlGraphics;
use daemon::config::Config;
use daemon::ctrl_anime::CtrlAnimeDisplay;
use daemon::ctrl_charge::CtrlCharge;
use daemon::ctrl_fan_cpu::{CtrlFanAndCPU, DbusFanAndCpu};
use daemon::ctrl_leds::{CtrlKbdBacklight, DbusKbdBacklight};
use daemon::laptops::match_laptop;

use asus_nb::DBUS_NAME;
use daemon::{CtrlTask, Reloadable, ZbusAdd};
use log::LevelFilter;
use log::{error, info, warn};
use std::error::Error;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

use std::convert::Into;
use std::convert::TryInto;
use zbus::fdo;
use zbus::Connection;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut logger = env_logger::Builder::new();
    logger
        .target(env_logger::Target::Stdout)
        .format(|buf, record| writeln!(buf, "{}: {}", record.level(), record.args()))
        .filter(None, LevelFilter::Info)
        .init();

    info!("Version: {}", daemon::VERSION);
    start_daemon()?;
    Ok(())
}

// Timing is such that:
// - interrupt write is minimum 1ms (sometimes lower)
// - read interrupt must timeout, minimum of 1ms
// - for a single usb packet, 2ms total.
// - to maintain constant times of 1ms, per-key colours should use
//   the effect endpoint so that the complete colour block is written
//   as fast as 1ms per row of the matrix inside it. (10ms total time)
//
// DBUS processing takes 6ms if not tokiod
fn start_daemon() -> Result<(), Box<dyn Error>> {
    let laptop = match_laptop();
    let config = if let Some(laptop) = laptop.as_ref() {
        Config::default().load(laptop.supported_modes())
    } else {
        Config::default().load(&[])
    };

    let connection = Connection::new_system()?;
    fdo::DBusProxy::new(&connection)?
        .request_name(DBUS_NAME, fdo::RequestNameFlags::ReplaceExisting.into())?;
    let mut object_server = zbus::ObjectServer::new(&connection);

    let enable_gfx_switching = config.gfx_managed;
    let config = Arc::new(Mutex::new(config));

    match CtrlCharge::new(config.clone()) {
        Ok(mut ctrl) => {
            // Do a reload of any settings
            ctrl.reload()
                .unwrap_or_else(|err| warn!("Battery charge limit: {}", err));
            // Then register to dbus server
            ctrl.add_to_server(&mut object_server);
        }
        Err(err) => {
            error!("charge_control: {}", err);
        }
    }

    match CtrlAnimeDisplay::new() {
        Ok(ctrl) => {
            ctrl.add_to_server(&mut object_server);
        }
        Err(err) => {
            error!("AniMe control: {}", err);
        }
    }

    if enable_gfx_switching {
        match CtrlGraphics::new() {
            Ok(mut ctrl) => {
                ctrl.reload()
                    .unwrap_or_else(|err| warn!("Gfx controller: {}", err));
                ctrl.add_to_server(&mut object_server);
            }
            Err(err) => {
                error!("Gfx control: {}", err);
            }
        }
    }

    // Collect tasks for task thread
    let mut tasks: Vec<Arc<Mutex<dyn CtrlTask + Send>>> = Vec::new();

    match CtrlFanAndCPU::new(config.clone()) {
        Ok(mut ctrl) => {
            ctrl.reload()
                .unwrap_or_else(|err| warn!("Profile control: {}", err));
            let tmp = Arc::new(Mutex::new(ctrl));
            DbusFanAndCpu::new(tmp.clone()).add_to_server(&mut object_server);
            tasks.push(tmp);
        }
        Err(err) => {
            error!("Profile control: {}", err);
        }
    };

    if let Some(laptop) = laptop {
        let ctrl = CtrlKbdBacklight::new(
            laptop.usb_product(),
            laptop.condev_iface(),
            laptop.supported_modes().to_owned(),
            config,
        );
        let tmp = Arc::new(Mutex::new(ctrl));
        DbusKbdBacklight::new(tmp.clone()).add_to_server(&mut object_server);
        tasks.push(tmp);
    }

    // TODO: implement messaging between threads to check fails
    // These tasks generally read a sys path or file to check for a
    // change
    let _handle = std::thread::Builder::new()
        .name("asusd watch".to_string())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));

            for ctrl in tasks.iter() {
                if let Ok(mut lock) = ctrl.try_lock() {
                    lock.do_task()
                        .map_err(|err| {
                            warn!("do_task error: {}", err);
                        })
                        .ok();
                }
            }
        });

    object_server.with(&"/org/asuslinux/Charge".try_into()?, |obj: &CtrlCharge| {
        let x = obj.limit();
        obj.notify_charge(x as u8)
    })?;

    loop {
        if let Err(err) = object_server.try_handle_next() {
            eprintln!("{}", err);
        }
    }
}