use crate::tr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub address: String,
    pub name: String,
    pub connected: bool,
    pub is_audio: bool,
}

#[derive(Default)]
pub struct Update {
    pub devices: Option<Vec<Device>>,
    pub error: Option<String>,
}

pub fn matching(devices: &[Device], query: &str) -> Vec<Device> {
    let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut devices: Vec<_> = devices
        .iter()
        .filter(|device| {
            let text = format!("{} {}", device.name, device.address).to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
        .cloned()
        .collect();
    devices.sort_by_cached_key(|device| {
        (
            !device.is_audio,
            !device.connected,
            device.name.to_lowercase(),
            device.address.clone(),
        )
    });
    devices
}

pub trait Connection {
    fn connected(&self) -> bool;
    fn set_connected(&self, connected: bool) -> Result<(), String>;
}

/// Apply the state requested by the visible row, even if its cached state is stale.
pub fn apply_connection(connection: &impl Connection, connected: bool) -> Result<(), String> {
    if connection.connected() == connected {
        return Ok(());
    }
    let result = connection.set_connected(connected);
    if connection.connected() == connected {
        Ok(())
    } else {
        result.and(Err(tr!(
            "设备连接状态未改变，请按 ⌘R 刷新后重试。",
            "The device connection did not change. Press ⌘R to refresh and retry."
        )
        .into()))
    }
}
