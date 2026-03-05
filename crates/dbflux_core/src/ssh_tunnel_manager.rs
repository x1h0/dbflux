use crate::SshTunnelProfile;
use crate::item_manager::{DefaultFilename, ItemManager};

pub type SshTunnelManager = ItemManager<SshTunnelProfile>;

impl DefaultFilename for SshTunnelManager {
    fn meta() -> (&'static str, &'static str) {
        ("ssh_tunnels.json", "SSH tunnel profiles")
    }
}
