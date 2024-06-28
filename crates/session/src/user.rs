use std::collections::{BTreeSet, HashMap, HashSet};
use serde::{Deserialize, Serialize};
use dce_util::mixed::{DceErr, DceResult};
use crate::session::Session;
#[cfg(feature = "async")]
use async_trait::async_trait;

const DEFAULT_USER_PREFIX: &str = "dceusmap";
const DEFAULT_USER_FIELD: &str = "$user";
pub const MAPPING_TTL_SECONDS: i64 = 60 * 60 * 24 * 7;

#[derive(Clone)]
pub struct UserMeta<U: UidGetter + Serialize> {
    key_prefix: &'static str,
    user_field: &'static str,
    try_loaded: bool,
    user: Option<U>,
}

impl<U: UidGetter + Serialize> UserMeta<U> {
    pub fn key_prefix(&self) -> &str {
        self.key_prefix
    }

    pub fn user_field(&self) -> &str {
        self.user_field
    }

    pub fn user(&self) -> Option<&U> {
        self.user.as_ref()
    }

    pub fn set_user(&mut self, user: Option<U>) {
        self.user = user;
    }

    pub fn config(
        &mut self,
        key_prefix: Option<&'static str>,
        user_field: Option<&'static str>,
    ) {
        if let Some(key_prefix) = key_prefix { self.key_prefix = key_prefix; }
        if let Some(user_field) = user_field { self.user_field = user_field; }
    }

    pub fn new() -> Self {
        Self {
            key_prefix: DEFAULT_USER_PREFIX,
            user_field: DEFAULT_USER_FIELD,
            try_loaded: false,
            user: None,
        }
    }
}

macro_rules! auto_async {
    { $($(#[$($meta:meta),+])+, $($async: ident)?);+ } => {
        #[cfg_attr(feature = "async", async_trait)]
        pub trait User<U: UidGetter + Serialize>: Session {
            fn user_meta(&self) -> &UserMeta<U>;
        
            fn user_meta_mut(&mut self) -> &mut UserMeta<U>;
        
            fn gen_user_key(user_prefix: &str, id: u64) -> String;
        
            $($(#[$($meta),+])+
            $($async)? fn mapping(&mut self) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn unmapping(&mut self) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn sync(&mut self, user: &U) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn all_sid(&mut self, uid: u64) -> DceResult<HashSet<String>>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn filter_sids(&mut self, uid: u64, sids: BTreeSet<String>) -> DceResult<BTreeSet<String>>; )+

            $($(#[$($meta),+])+
            $($async)? fn user_key(&mut self) -> DceResult<String> where U: Send + for<'a> Deserialize<'a> {
                #[cfg(feature = "async")]
                return Ok(Self::gen_user_key(self.user_meta().key_prefix, self.uid().await?));
                #[cfg(not(feature = "async"))]
                return Ok(Self::gen_user_key(self.user_meta().key_prefix, self.uid()?));
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn user(&mut self) -> Option<&U>
                where U: Send + for<'a> Deserialize<'a>,
            {
                // just try load once
                if self.user_meta().try_loaded {
                    return self.user_meta().user();
                }
                let user_field = self.user_meta().user_field;
                #[cfg(feature = "async")]
                if let Ok(Ok(user)) = self.silent_get(user_field).await.map(|us| serde_json::from_str::<U>(&us)) {
                    self.user_meta_mut().set_user(Some(user));
                }
                #[cfg(not(feature = "async"))]
                if let Ok(Ok(user)) = self.silent_get(user_field).map(|us| serde_json::from_str::<U>(&us)) {
                    self.user_meta_mut().set_user(Some(user));
                }
                self.user_meta_mut().try_loaded = true;
                self.user_meta().user()
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn uid(&mut self) -> DceResult<u64> where U: Send + for<'a> Deserialize<'a> {
                #[cfg(feature = "async")]
                return self.user().await.map(|u| u.id()).ok_or_else(|| DceErr::closed0("Not a logged session cannot take uid"));
                #[cfg(not(feature = "async"))]
                return self.user().map(|u| u.id()).ok_or_else(|| DceErr::closed0("Not a logged session cannot take uid"));
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn log_in(&mut self, user: Option<U>, ttl_minutes: Option<u16>) -> DceResult<bool> where
                U: Send + 'static,
                Self: Clone + Send
            {
                *self.cloned_mut() = Some(Box::new(self.clone()));
                self.meta_mut().config(None, ttl_minutes);
                if user.is_some() { self.user_meta_mut().set_user(user); }
                let mut filters = HashMap::new();
                if let Some(user) = self.user_meta().user() {
                    filters.insert(self.user_meta().user_field().to_string(), serde_json::to_string(user).ok());
                }
                #[cfg(feature = "async")]
                return match self.renew(filters).await {
                    // must use the new session when login, so just delete old here
                    Ok(_) => self.cloned_destroy().await,
                    result => result,
                };
                #[cfg(not(feature = "async"))]
                return match self.renew(filters) {
                    // must use the new session when login, so just delete old here
                    Ok(_) => self.cloned_destroy(),
                    result => result,
                };
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn login(&mut self, user: U, ttl_minutes: u16) -> DceResult<bool> where
                U: Send + 'static,
                Self: Clone + Send
            {
                #[cfg(feature = "async")]
                return self.log_in(Some(user), Some(ttl_minutes)).await;
                #[cfg(not(feature = "async"))]
                return self.log_in(Some(user), Some(ttl_minutes));
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn auto_login(&mut self) -> DceResult<bool> where
                U: Send + 'static,
                Self: Clone + Send
            {
                #[cfg(feature = "async")]
                return self.log_in(None, None).await;
                #[cfg(not(feature = "async"))]
                return self.log_in(None, None);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn logout(&mut self) -> DceResult<bool> where
                U: Send + for<'a> Deserialize<'a> + 'static,
            {
                #[cfg(feature = "async")]
                if self.user().await.is_some() {
                    self.unmapping().await?;
                    self.user_meta_mut().set_user(None);
                    let user_field = self.user_meta().user_field().to_string();
                    return self.silent_del(user_field.as_str()).await;
                }
                #[cfg(not(feature = "async"))]
                if self.user().is_some() {
                    self.unmapping()?;
                    self.user_meta_mut().set_user(None);
                    let user_field = self.user_meta().user_field().to_string();
                    return self.silent_del(user_field.as_str());
                }
                Ok(false)
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn sids(&mut self, uid: u64) -> DceResult<BTreeSet<String>> {
                #[cfg(feature = "async")]
                let sids = self.all_sid(uid).await?;
                #[cfg(feature = "async")]
                return self.filter_sids(uid, sids.into_iter().collect::<BTreeSet<_>>()).await;
                #[cfg(not(feature = "async"))]
                let sids = self.all_sid(uid)?;
                #[cfg(not(feature = "async"))]
                return self.filter_sids(uid, sids.into_iter().collect::<BTreeSet<_>>());
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_unmapping(&mut self) -> DceResult<bool>; )+
        }
        
        $($(#[$($meta),+])+             
        pub $($async)? fn renew<U: UidGetter + Serialize, S: User<U> + Send>(session: &mut S, filters: HashMap<String, Option<String>>) -> DceResult<bool> {
            #[cfg(feature = "async")]
            let result = super::session::renew(session, filters).await?;
            #[cfg(not(feature = "async"))]
            let result = super::session::renew(session, filters)?;
            session.user_meta_mut().try_loaded = false;
            #[cfg(feature = "async")]
            let _ = session.mapping().await;
            #[cfg(not(feature = "async"))]
            let _ = session.mapping();
            Ok(result)
        } )+
    };
}
auto_async! {#[cfg(feature = "async")], async; #[cfg(not(feature = "async"))],}

pub fn clone_with_id<U: UidGetter + Serialize, S: User<U> + Clone>(session: &mut S, id: String) -> DceResult<S> {
    let mut cloned = super::session::clone_with_id(session, id)?;
    cloned.user_meta_mut().user = None;
    cloned.user_meta_mut().try_loaded = false;
    Ok(cloned)
}


pub trait UidGetter {
    fn id(&self) -> u64;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SessionUser {
    id: u64,
    role_id: u16,
    nick: String,
}

impl UidGetter for SessionUser {
    fn id(&self) -> u64 {
        self.id
    }
}
