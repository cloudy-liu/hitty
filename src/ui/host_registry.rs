use std::collections::HashMap;
use std::sync::{Arc, Weak};

use gpui::{App, Global};
use tty7_core::host::local::LocalHost;

pub use tty7_core::host::{Host, HostId, SharedHost};

pub struct HostRegistry {
    hosts: HashMap<HostId, SharedHost>,
    pane_hosts: HashMap<HostId, Weak<dyn Host>>,
}

impl Global for HostRegistry {}

impl Default for HostRegistry {
    fn default() -> Self {
        let mut hosts = HashMap::new();
        hosts.insert(HostId::LOCAL, LocalHost::shared());
        HostRegistry {
            hosts,
            pane_hosts: HashMap::new(),
        }
    }
}

impl HostRegistry {
    pub fn get(cx: &mut App, id: HostId) -> Option<SharedHost> {
        cx.default_global::<HostRegistry>();
        Self::lookup(cx, id)
    }

    pub fn lookup(cx: &App, id: HostId) -> Option<SharedHost> {
        match cx.try_global::<HostRegistry>() {
            Some(reg) => reg.hosts.get(&id).cloned().or_else(|| {
                reg.pane_hosts
                    .get(&id)?
                    .upgrade()
                    .filter(|host| host.is_connected())
            }),
            None => id.is_local().then(LocalHost::shared),
        }
    }

    pub fn local(cx: &mut App) -> SharedHost {
        HostRegistry::get(cx, HostId::LOCAL).expect("the local host is always registered")
    }

    pub fn insert(cx: &mut App, host: SharedHost) -> Option<SharedHost> {
        let id = host.id();
        cx.default_global::<HostRegistry>().hosts.insert(id, host)
    }

    /// Borrow a pane route for Git lookup without registering another machine.
    /// The pane owns the adapter; open views cannot keep its connection alive.
    pub(crate) fn insert_pane(cx: &mut App, host: &SharedHost) {
        let reg = cx.default_global::<HostRegistry>();
        reg.pane_hosts
            .retain(|_, weak| weak.upgrade().is_some_and(|host| host.is_connected()));
        reg.pane_hosts.insert(host.id(), Arc::downgrade(host));
    }

    /// Track the borrowed pane's lifetime without keeping its route alive.
    pub(crate) fn pane_lifetime(cx: &App, id: HostId) -> Option<Weak<dyn Host>> {
        cx.try_global::<Self>()?.pane_hosts.get(&id).cloned()
    }

    pub(crate) fn remove_pane(cx: &mut App, id: HostId) {
        cx.default_global::<HostRegistry>().pane_hosts.remove(&id);
    }

    pub fn remove(cx: &mut App, id: HostId) -> Option<SharedHost> {
        if id.is_local() {
            return None;
        }
        cx.default_global::<HostRegistry>().hosts.remove(&id)
    }

    pub fn ids(cx: &mut App) -> Vec<HostId> {
        let mut ids: Vec<HostId> = cx
            .default_global::<HostRegistry>()
            .hosts
            .keys()
            .copied()
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn len(cx: &mut App) -> usize {
        cx.default_global::<HostRegistry>().hosts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_registry_already_holds_the_local_host() {
        let reg = HostRegistry::default();
        let local = reg.hosts.get(&HostId::LOCAL).expect("local is present");
        assert!(local.id().is_local());
        assert_eq!(reg.hosts.len(), 1);
    }

    #[test]
    fn remote_hosts_come_and_go_but_local_stays() {
        let mut reg = HostRegistry::default();
        let id = HostId::from_connection_key("ssh-direct:me@box:22");
        reg.hosts.insert(id, LocalHost::new());
        assert_eq!(reg.hosts.len(), 2);

        assert!(reg.hosts.remove(&id).is_some());
        assert!(reg.hosts.contains_key(&HostId::LOCAL));
        assert!(reg.hosts.get(&id).is_none());
    }
}
