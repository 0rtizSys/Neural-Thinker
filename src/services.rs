//! Extension seam for optional services.
//!
//! The open-source (Community) build runs with no services at all. Paid
//! services such as cloud storage and cross-device sync are developed in a
//! separate, private crate that is never committed to this repository (see
//! `docs/PRIVATE_SERVICES.md`). That crate depends on this one and passes its
//! services to [`crate::run`]; nothing in this crate refers to it.

use std::path::Path;

use eframe::egui;

/// A change to a file or folder inside the root folder, reported to every service.
#[derive(Clone, Copy, Debug)]
pub enum VaultEvent<'a> {
    /// A note was written to disk.
    Saved(&'a Path),
    /// A file or folder was created from the navigation bar.
    Created(&'a Path),
    Renamed {
        from: &'a Path,
        to: &'a Path,
    },
    Deleted(&'a Path),
    /// The user picked a different root folder.
    RootChanged(&'a Path),
}

/// An optional service plugged into the application.
pub trait Service {
    /// Short name shown in the Services window, e.g. "Cloud sync".
    fn name(&self) -> &str;

    /// One-line state shown next to the name, e.g. "Signed in" or "Offline".
    fn status(&self) -> String {
        String::new()
    }

    /// Settings UI drawn in the Services window.
    fn settings_ui(&mut self, _ui: &mut egui::Ui) {}

    /// Called once per frame, for polling background work.
    fn update(&mut self, _ctx: &egui::Context) {}

    /// Called after a change to the root folder.
    fn on_vault_event(&mut self, _event: VaultEvent<'_>) {}
}

/// The services this build runs with.
pub struct Services {
    edition: String,
    list: Vec<Box<dyn Service>>,
}

impl Services {
    /// The open-source build: no services.
    pub fn community() -> Self {
        Self {
            edition: "Community".to_owned(),
            list: Vec::new(),
        }
    }

    /// A build with its own services, under the given edition name.
    pub fn new(edition: impl Into<String>, list: Vec<Box<dyn Service>>) -> Self {
        Self {
            edition: edition.into(),
            list,
        }
    }

    pub fn edition(&self) -> &str {
        &self.edition
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Service>> {
        self.list.iter_mut()
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        for service in &mut self.list {
            service.update(ctx);
        }
    }

    pub fn notify(&mut self, event: VaultEvent<'_>) {
        for service in &mut self.list {
            service.on_vault_event(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Recorder(Rc<RefCell<Vec<String>>>);

    impl Service for Recorder {
        fn name(&self) -> &str {
            "recorder"
        }
        fn on_vault_event(&mut self, event: VaultEvent<'_>) {
            self.0.borrow_mut().push(format!("{event:?}"));
        }
    }

    #[test]
    fn community_build_has_no_services() {
        let services = Services::community();
        assert!(services.is_empty());
        assert_eq!(services.edition(), "Community");
    }

    #[test]
    fn events_reach_every_service() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut services = Services::new(
            "Test",
            vec![
                Box::new(Recorder(Rc::clone(&log))),
                Box::new(Recorder(Rc::clone(&log))),
            ],
        );
        services.notify(VaultEvent::Saved(Path::new("a.md")));
        assert_eq!(log.borrow().len(), 2);
        assert!(log.borrow()[0].contains("a.md"));
    }
}
