use std::sync::{Arc, Weak};
use dce_util::mixed::{DceErr, DceResult};
use crate::session::Session;
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use futures::lock::Mutex;
#[cfg(not(feature = "async"))]
use std::sync::Mutex;

const DEFAULT_SERVER_FIELD: &str = "$server";

#[derive(Clone)]
pub struct ConnectionMeta<T: Connection> {
    server_field: &'static str,
    shadow: Weak<Mutex<T>>,
    /// - Some(Some(_)): root and server addr unbound, and means first request of connection when session not cloned
    /// - Some(None): root and server addr bounded
    /// - None: session of request
    server_unbound: Option<Option<String>>,
}

impl<T: Connection> ConnectionMeta<T> {
    pub fn server_field(&self) -> &'static str {
        self.server_field
    }
    
    pub fn server_unbound(&self) -> &Option<Option<String>> {
        &self.server_unbound
    }
    
    pub fn new() -> Self {
        Self { server_field: DEFAULT_SERVER_FIELD, shadow: Default::default(), server_unbound: None }
    }
}

#[cfg_attr(feature = "async", async_trait)]
pub trait Connection: Session {
    fn conn_meta(&self) -> &ConnectionMeta<Self>;

    fn conn_meta_mut(&mut self) -> &mut ConnectionMeta<Self>;
    
    #[cfg(feature = "async")]
    async fn unbinding(&mut self) -> DceResult<bool>;

    #[cfg(not(feature = "async"))]
    fn unbinding(&mut self) -> DceResult<bool>;
    
    fn connect(mut self, server: String) -> Arc<Mutex<Self>> {
        self.conn_meta_mut().server_unbound = Some(Some(server));
        Arc::new(Mutex::new(self))
    }

    #[cfg(feature = "async")]
    async fn disconnect(&mut self) -> DceResult<bool> {
        return self.unbinding().await;
    }

    #[cfg(not(feature = "async"))]
    fn disconnect(mut self) -> DceResult<bool> {
        return self.unbinding();
    }

    #[cfg(feature = "async")]
    async fn clone_for_request(root: Weak<Mutex<Self>>, sid: Option<String>) -> DceResult<Self> where Self: Clone {
        let arc = root.upgrade().ok_or_else(|| DceErr::closed0("Failed to upgrade to Arc"))?;
        let mut guard = arc.lock().await;
        let sid = sid.unwrap_or_else(|| guard.id().to_string());
        guard.clone_with_id(sid).map(|mut session| {
            session.conn_meta_mut().server_unbound = None;
            session.conn_meta_mut().shadow = root;
            session
        })
    }

    #[cfg(not(feature = "async"))]
    fn clone_for_request(root: Weak<Mutex<Self>>, sid: Option<String>) -> DceResult<Self> where Self: Clone {
        let arc = root.upgrade().ok_or_else(|| DceErr::closed0("Failed to upgrade to Arc"))?;
        let mut guard = arc.lock().map_err(DceErr::closed0);
        let sid = sid.unwrap_or_else(|| guard.id().to_string());
        guard.clone_with_id(sid).map(|mut session| {
            session.conn_meta_mut().server_unbound = None;
            session.conn_meta_mut().shadow = root;
            session
        })
    }

    #[cfg(feature = "async")]
    async fn update_shadow(&mut self, new_sid: String) -> DceResult<()> where Self: Send {
        let lock = self.conn_meta().shadow.upgrade().ok_or_else(|| DceErr::closed0("Not the cloned connection session"))?;
        let mut lock = lock.lock().await;
        if let Some(Some(server)) = lock.conn_meta().server_unbound() {
            let _ = self.silent_set(self.conn_meta().server_field, server.as_str()).await;
            lock.conn_meta_mut().server_unbound = Some(None);
        }
        lock.meta_mut().renew(Some(new_sid))
    }

    #[cfg(not(feature = "async"))]
    fn update_shadow(&mut self, new_sid: String) -> DceResult<()> where Self: Send {
        let lock = self.conn_meta().shadow.upgrade().ok_or_else(|| DceErr::closed0("Not the cloned connection session"))?;
        let mut lock = lock.lock().map_err(DceErr::closed0);
        if let Some(Some(server)) = lock.conn_meta().server_unbound() {
            let _ = self.silent_set(self.conn_meta().server_field, server.as_str());
            lock.conn_meta_mut().server_unbound = Some(None);
        }
        lock.meta_mut().renew(Some(new_sid))
    }
}
