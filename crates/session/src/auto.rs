use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::time::{SystemTime, UNIX_EPOCH};
use dce_util::mixed::{DceErr, DceResult};
use crate::session::Session;

const DEFAULT_NEW_SID_FIELD: &str = "$newid";
const DEFAULT_RENEW_INTERVAL_SECONDS: u16 = 600;
const DEFAULT_ORIGINAL_JUDGMENT_SECONDS: u16 = 120;
const DEFAULT_CLONED_INACTIVE_JUDGMENT_SECONDS: u16 = 60;

#[derive(Clone)]
pub struct AutoRenew<S: Session> {
    session: S,
    new_sid_field: &'static str,
    renew_interval_seconds: u16,
    original_judgment_seconds: u16,
    cloned_inactive_judgment_seconds: u16,
}

impl<S: Session + Send + Clone> AutoRenew<S> {
    pub fn new(session: S) -> Self {
        Self {
            session,
            new_sid_field: DEFAULT_NEW_SID_FIELD,
            renew_interval_seconds: DEFAULT_RENEW_INTERVAL_SECONDS,
            original_judgment_seconds: DEFAULT_ORIGINAL_JUDGMENT_SECONDS,
            cloned_inactive_judgment_seconds: DEFAULT_CLONED_INACTIVE_JUDGMENT_SECONDS,
        }
    }
    
    pub fn config(mut self, renew_interval_seconds: Option<u16>, original_judgment_seconds: Option<u16>, cloned_inactive_judgment_seconds: Option<u16>, new_sid_field: Option<&'static str>) -> Self {
        if let Some(v) = renew_interval_seconds { self.renew_interval_seconds = v }
        if let Some(v) = original_judgment_seconds { self.original_judgment_seconds = v }
        if let Some(v) = cloned_inactive_judgment_seconds { self.cloned_inactive_judgment_seconds = v }
        if let Some(v) = new_sid_field { self.new_sid_field = v }
        self
    }

    pub fn unwrap(self) -> S {
        self.session
    }
    
    #[cfg(feature = "async")]
    async fn do_clone(&mut self, filters: HashMap<String, Option<String>>) -> DceResult<bool> where Self: Clone {
        *self.cloned_mut() = Some(Box::new(self.session.clone()));
        match self.renew(filters).await {
            // log new sid in old session
            Ok(_) => {
                let _ = self.cloned_touch().await;
                let (sid, new_sid_name) = (self.id().to_string(), self.new_sid_field.to_string());
                self.cloned_silent_set(new_sid_name.as_str(), sid.as_str()).await
            },
            result => result,
        }
    }

    #[cfg(not(feature = "async"))]
    fn do_clone(&mut self, filters: HashMap<String, Option<String>>) -> DceResult<bool> where Self: Clone {
        *self.cloned_mut() = Some(Box::new(self.session.clone()));
        match self.renew(filters) {
            Ok(_) => {
                let _ = self.cloned_touch();
                let (sid, new_sid_name) = (self.id().to_string(), self.new_sid_field.to_string());
                self.cloned_silent_set(new_sid_name.as_str(), sid.as_str())
            },
            result => result,
        }
    }

    /// Returns true if cloned a new session, or false keep use old
    #[cfg(feature = "async")]
    pub async fn try_renew(&mut self) -> DceResult<bool> where Self: Clone {
        let seconds_from_renew = SystemTime::now().duration_since(UNIX_EPOCH).map_err(DceErr::closed0)?.as_secs() as isize
            - self.meta().create_stamp() as isize - self.renew_interval_seconds as isize;
        // if not time to renew then touch the old and return false
        if seconds_from_renew < 0 {
            let _ = self.try_touch().await;
            return Ok(false);
        }
        // if got new sid on old session then try to do the old and new maintenance work
        // or just make a new clone of old
        let new_sid_field = self.new_sid_field.to_string();
        if let Ok(new_sid) = self.silent_get(new_sid_field.as_str()).await {
            // if time from renew large than original_judgment_seconds then do old or new destroy work
            if seconds_from_renew > self.original_judgment_seconds as isize {
                *self.cloned_mut() = self.clone_with_id(new_sid).map(Box::new).ok();
                let new_tp = self.cloned_ttl_passed().await.unwrap_or(u32::MAX);
                // destroy current old session if newer is active
                // or try to delete the newer and make a new clone of old
                return if new_tp < self.cloned_inactive_judgment_seconds as u32
                    && self.ttl_passed().await.map_or(true, |old_tp| new_tp < old_tp)
                {
                    self.destroy().await?;
                    DceErr::closed0_wrap(format!(r#"session "{}" was destroied, unable to continue use"#, self.id()))
                } else {
                    self.cloned_destroy().await?;
                    self.do_clone(HashMap::from([(new_sid_field, None)])).await
                }
            }
            // or just do touch
            let _ = self.try_touch().await;
            Ok(false)
        } else {
            self.do_clone(HashMap::new()).await
        }
    }

    #[cfg(not(feature = "async"))]
    pub fn try_renew(&mut self) -> DceResult<bool> where Self: Clone {
        let seconds_from_renew = SystemTime::now().duration_since(UNIX_EPOCH).map_err(DceErr::closed0)?.as_secs() as isize
            - self.meta().create_stamp() as isize - self.renew_interval_seconds as isize;
        if seconds_from_renew < 0 {
            let _ = self.try_touch();
            return Ok(false);
        }
        let new_sid_field = self.new_sid_field.to_string();
        if let Ok(new_sid) = self.silent_get(new_sid_field.as_str()) {
            if seconds_from_renew > self.original_judgment_seconds as isize {
                *self.cloned_mut() = self.clone_with_id(new_sid).map(Box::new).ok();
                let new_tp = self.cloned_ttl_passed().unwrap_or(u32::MAX);
                return if new_tp < self.cloned_inactive_judgment_seconds as u32
                    && self.ttl_passed().map_or(true, |old_tp| new_tp < old_tp)
                {
                    self.destroy()?;
                    DceErr::closed0_wrap(format!(r#"session "{}" was destroied, unable to continue use"#, self.id()))
                } else {
                    self.cloned_destroy()?;
                    self.do_clone(HashMap::from([(new_sid_field, None)]))
                }
            }
            let _ = self.try_touch();
            Ok(false)
        } else {
            self.do_clone(HashMap::new())
        }
    }
}

impl<S: Session> Deref for AutoRenew<S> where {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl<S: Session> DerefMut for AutoRenew<S> where {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}
