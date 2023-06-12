// jkcoxson

use crate::devices::SharedDevices;
use log::info;
use std::net::IpAddr;
use std::sync::Arc;

use tokio::sync::Mutex;

#[cfg(not(feature = "zeroconf"))]
use {
    futures_util::{pin_mut, stream::StreamExt},
    mdns::{Record, RecordKind},
    std::time::Duration,
};

#[cfg(feature = "zeroconf")]
use {
    zeroconf::prelude::*,
    zeroconf::{MdnsBrowser, ServiceType},
};

const SERVICE_NAME: &str = "apple-mobdev2";
const SERVICE_NAME_PAIRABLE: &str = "apple-pairable";
const SERVICE_PROTOCOL: &str = "tcp";

#[cfg(feature = "zeroconf")]
pub async fn discover(data: Arc<Mutex<SharedDevices>>) {
    let service_name = format!("_{}._{}.local", SERVICE_NAME, SERVICE_PROTOCOL);
    let service_name_pairable = format!("_{}._{}.local", SERVICE_NAME_PAIRABLE, SERVICE_PROTOCOL);
    println!(
        "Starting mDNS discovery for {},{} with zeroconf",
        service_name, service_name_pairable
    );

    let mut browser = MdnsBrowser::new(ServiceType::new(SERVICE_NAME, SERVICE_PROTOCOL).unwrap());
    let mut browser_pairable =
        MdnsBrowser::new(ServiceType::new(SERVICE_NAME_PAIRABLE, SERVICE_PROTOCOL).unwrap());
    loop {
        let result;
        tokio::select! {
            v = browser.browse_async() => result = v,
            v = browser_pairable.browse_async() =>  result = v
        };

        if let Ok(service) = result {
            info!("Service discovered: {:?}", service);
            let name = service.name();
            if !name.contains("@") {
                continue;
            }
            let addr = match service.address() {
                addr if addr.contains(":") => IpAddr::V6(addr.parse().unwrap()),
                addr => IpAddr::V4(addr.parse().unwrap()),
            };

            let mac_addr = name.split("@").collect::<Vec<&str>>()[0];
            let mut lock = data.lock().await;
            let pairable = service.service_type().name() == SERVICE_NAME_PAIRABLE;
            if let Ok(udid) = lock.get_udid_from_mac(mac_addr.to_string(), pairable) {
                if lock.devices.contains_key(&udid) {
                    info!("Device has already been added to muxer, skipping");
                    continue;
                }
                println!("Adding device {}", udid);

                lock.add_network_device(
                    udid,
                    addr,
                    service_name.clone(),
                    "Network".to_string(),
                    pairable,
                    data.clone(),
                )
            }
        }
    }
}

#[cfg(not(feature = "zeroconf"))]
pub async fn discover(data: Arc<Mutex<SharedDevices>>) {
    let service_name = format!("_{}._{}.local", SERVICE_NAME, SERVICE_PROTOCOL);
    let service_name_pairable = format!("_{}._{}.local", SERVICE_NAME_PAIRABLE, SERVICE_PROTOCOL);
    let service_names = vec![service_name.clone(), service_name_pairable.clone()];

    println!(
        "Starting mDNS discovery for {} with mdns",
        service_names.join(", ")
    );

    let stream = mdns::discover::all(service_names, Duration::from_secs(5))
        .unwrap()
        .listen();
    pin_mut!(stream);

    while let Some(Ok(response)) = stream.next().await {
        let addr = response.records().filter_map(self::to_ip_addr).next();

        if let Some(mut addr) = addr {
            let mut mac_addr = None;
            let mut service_type = None;
            let mut name = None;
            for i in response.records() {
                if let RecordKind::A(addr4) = i.kind {
                    addr = std::net::IpAddr::V4(addr4)
                }
                if i.name.contains(&service_name) && i.name.contains('@') {
                    mac_addr = Some(i.name.split('@').collect::<Vec<&str>>()[0]);
                    service_type = Some(SERVICE_NAME)
                }
                if i.name.contains(&service_name_pairable) && i.name.contains('@') {
                    mac_addr = Some(i.name.split('@').collect::<Vec<&str>>()[0]);
                    service_type = Some(SERVICE_NAME_PAIRABLE)
                }
                name = Some(i.name.clone())
            }

            // Look through paired devices for mac address
            if mac_addr.is_none() || name.is_none() {
                continue;
            }
            info!("Service discovered: {:?}", name.unwrap());
            let mac_addr = mac_addr.unwrap();
            let mut lock = data.lock().await;
            let pairable = service_type.unwrap() == SERVICE_NAME_PAIRABLE;
            if let Ok(udid) = lock.get_udid_from_mac(mac_addr.to_string(), pairable) {
                if lock.devices.contains_key(&udid) {
                    info!("Device has already been added to muxer, skipping");
                    continue;
                }
                println!("Adding device {}", udid);

                lock.add_network_device(
                    udid,
                    addr,
                    service_name.clone(),
                    "Network".to_string(),
                    pairable,
                    data.clone(),
                )
            }
        }
    }
}

#[cfg(not(feature = "zeroconf"))]
fn to_ip_addr(record: &Record) -> Option<IpAddr> {
    match record.kind {
        RecordKind::A(addr) => Some(addr.into()),
        RecordKind::AAAA(addr) => Some(addr.into()),
        _ => None,
    }
}
