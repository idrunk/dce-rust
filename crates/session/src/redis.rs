use std::collections::{BTreeSet, HashMap, HashSet};
use log::warn;
use serde::{Deserialize, Serialize};
use dce_util::mixed::{DceErr, DceResult};
use crate::session::{Session, Meta};
use crate::user::{MAPPING_TTL_SECONDS, UidGetter, SessionUser, User, UserMeta};
#[cfg(feature = "async")]
use async_trait::async_trait;
#[cfg(feature = "async")]
use redis::aio::ConnectionLike;
#[cfg(feature = "async")]
use redis::AsyncCommands;
#[cfg(not(feature = "async"))]
use redis::ConnectionLike;
#[cfg(not(feature = "async"))]
use redis::Commands;
#[cfg(feature = "connection")]
use crate::connection::{Connection, ConnectionMeta};
#[cfg(feature = "auto-renew")]
use crate::auto::AutoRenew;


pub type RedisBasic<R> = RedisSession<R, SessionUser>;

#[cfg(feature = "connection")]
pub struct RedisSession<R: ConnectionLike + Sync, U: UidGetter + Serialize> where Self: Connection {
    meta: Meta,
    user_meta: UserMeta<U>,
    conn_meta: ConnectionMeta<Self>,
    redis: Option<R>,
    cloned: Option<Box<Self>>,
}

#[cfg(feature = "connection")]
impl<R: ConnectionLike + Sync, U: UidGetter + Serialize + Clone> Clone for RedisSession<R, U> where Self: Connection {
    fn clone(&self) -> Self {
        Self{meta: self.meta.clone(), user_meta: self.user_meta.clone(), conn_meta: self.conn_meta.clone(), redis: None, cloned: None }
    }
}

#[cfg(not(feature = "connection"))]
pub struct RedisSession<R: ConnectionLike + Sync, U: UidGetter + Serialize> {
    meta: Meta,
    user_meta: UserMeta<U>,
    redis: Option<R>,
    cloned: Option<Box<Self>>,
}

#[cfg(not(feature = "connection"))]
impl<R: ConnectionLike + Sync, U: UidGetter + Serialize + Clone> Clone for RedisSession<R, U> {
    fn clone(&self) -> Self {
        Self{meta: self.meta.clone(), user_meta: self.user_meta.clone(), redis: None, cloned: None}
    }
}

impl<R, U> RedisSession<R, U> where
    R: ConnectionLike + Send + Sync, 
    U: UidGetter + Serialize + Send + Sync + for <'a> Deserialize<'a> + 'static,
{
    pub fn with(mut self, redis: R) -> Self {
        self.redis = Some(redis);
        self
    }

    pub fn redis_then(&mut self, redis: R) -> &mut Self {
        self.redis = Some(redis);
        self
    }

    #[cfg(feature = "auto-renew")]
    pub fn auto(self) -> AutoRenew<Self> where Self: Clone {
        AutoRenew::new(self)
    }
    
    fn redis(&mut self) -> DceResult<&mut R> {
        self.redis.as_mut().ok_or_else(|| DceErr::closed0("Not set the redis yet"))
    }
}


macro_rules! auto_async {
    { $($(#[$($meta:meta),+])+, $($async: ident)?);+ } => {

        #[cfg_attr(feature = "async", async_trait)]
        impl<R, U> Session for RedisSession<R, U> where
            R: ConnectionLike + Send + Sync,
            U: Serialize + UidGetter + Send + Sync + for<'a> Deserialize<'a> + 'static,
        {
            fn new(ttl_minutes: u16) -> DceResult<Self> {
                Meta::new(ttl_minutes).map(|meta| Self{meta, user_meta: UserMeta::new(), #[cfg(feature = "connection")] conn_meta: ConnectionMeta::new(), redis: None, cloned: None})
            }
            
            fn new_with_id(sid_pool: Vec<String>) -> DceResult<Self> {
                Meta::new_with_sid(sid_pool).map(|meta| Self{meta, user_meta: UserMeta::new(), #[cfg(feature = "connection")] conn_meta: ConnectionMeta::new(), redis: None, cloned: None})
            }
        
            fn meta(&self) -> &Meta {
                &self.meta
            }
        
            fn meta_mut(&mut self) -> &mut Meta {
                &mut self.meta
            }
            
            fn gen_key(sid_name: &str, id: &str) -> String {
                format!("{}:{}", sid_name, id)
            }
        
            $($(#[$($meta),+])+
            $($async)? fn silent_set(&mut self, field: &str, value: &str) -> DceResult<bool> {
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.hset(key, field, value).await.map(|_: bool| true).map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.hset(key, field, value).map(|_: bool| true).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn silent_get(&mut self, field: &str) -> DceResult<String> {
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.hget(key, field).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.hget(key, field).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn silent_del(&mut self, field: &str) -> DceResult<bool> {
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.hdel(key, field).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.hdel(key, field).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn destroy(&mut self) -> DceResult<bool> {
                #[cfg(feature = "async")]
                self.unmapping().await?;
                #[cfg(not(feature = "async"))]
                self.unmapping()?;
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.del(key).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.del(key).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn touch(&mut self) -> DceResult<bool> {
                let ttl_seconds = self.meta.ttl_seconds() as i64;
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.expire(key, ttl_seconds).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.expire(key, ttl_seconds).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn load(&mut self, data: HashMap<String, String>) -> DceResult<bool> {
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.hset_multiple(key, &data.into_iter().collect::<Vec<_>>()).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.hset_multiple(key, &data.into_iter().collect::<Vec<_>>()).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn raw(&mut self) -> DceResult<HashMap<String, String>> {
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.hgetall(key).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.hgetall(key).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn ttl_passed(&mut self) -> DceResult<u32> {
                let key = self.key();
                #[cfg(feature = "async")]
                return self.redis()?.ttl::<_, isize>(key).await.into_iter()
                    .find_map(|ttl| if ttl > 0 { Some(self.meta.ttl_seconds() - ttl as u32) } else { None })
                    .ok_or_else(|| DceErr::closed0("ttl was not initialized yet."));
                #[cfg(not(feature = "async"))]
                return self.redis()?.ttl::<_, isize>(key).into_iter()
                    .find_map(|ttl| if ttl > 0 { Some(self.meta.ttl_seconds() - ttl as u32) } else { None })
                    .ok_or_else(|| DceErr::closed0("ttl was not initialized yet."));
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn renew(&mut self, filters: HashMap<String, Option<String>>) -> DceResult<bool> {
                #[cfg(feature = "async")]
                let result = super::user::renew(self, filters).await;
                #[cfg(not(feature = "async"))]
                let result = super::user::renew(self, filters);
                #[cfg(feature = "connection")]
                if result.is_ok() && self.conn_meta.server_unbound().is_none() {
                    #[cfg(feature = "async")]
                    self.update_shadow(self.id().to_string()).await?;
                    #[cfg(not(feature = "async"))]
                    self.update_shadow(self.id().to_string())?;
                }
                result
            } )+
        
            fn clone_with_id(&mut self, id: String) -> DceResult<Self> where Self: Clone {
                super::user::clone_with_id(self, id)
            }
        
            fn cloned_mut(&mut self) -> &mut Option<Box<Self>> {
                &mut self.cloned
            }
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_silent_set(&mut self, field: &str, value: &str) -> DceResult<bool> {                
                let key = Self::gen_key(self.meta.sid_name(), self.cloned.as_ref().map(|c| c.id()).ok_or_else(|| DceErr::closed0("None cloned cannot get id"))?);
                #[cfg(feature = "async")]
                return self.redis()?.hset(key, field, value).await.map(|_: bool| true).map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.hset(key, field, value).map(|_: bool| true).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_destroy(&mut self) -> DceResult<bool> {                
                #[cfg(feature = "async")]
                self.cloned_unmapping().await?;
                #[cfg(not(feature = "async"))]
                self.cloned_unmapping()?;
                let key = Self::gen_key(self.meta.sid_name(), self.cloned.as_ref().map(|c| c.id()).ok_or_else(|| DceErr::closed0("None cloned cannot get id"))?);
                #[cfg(feature = "async")]
                self.redis()?.del(key).await.map_err(DceErr::closed0)?;
                #[cfg(not(feature = "async"))]
                self.redis()?.del(key).map_err(DceErr::closed0)?;
                // just return true, because old may not stored
                Ok(true)
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_touch(&mut self) -> DceResult<bool> {                
                let ttl_seconds = self.meta.ttl_seconds() as i64;
                let key = Self::gen_key(self.meta.sid_name(), self.cloned.as_ref().map(|c| c.id()).ok_or_else(|| DceErr::closed0("None cloned cannot get id"))?);
                #[cfg(feature = "async")]
                return self.redis()?.expire(key, ttl_seconds).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.expire(key, ttl_seconds).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_ttl_passed(&mut self) -> DceResult<u32> {
                let key = Self::gen_key(self.meta.sid_name(), self.cloned.as_ref().map(|c| c.id()).ok_or_else(|| DceErr::closed0("None cloned cannot get id"))?);
                #[cfg(feature = "async")]
                return self.redis()?.ttl::<_, isize>(key).await.into_iter()
                    .find_map(|ttl| if ttl > 0 { Some(self.meta.ttl_seconds() - ttl as u32) } else { None })
                    .ok_or_else(|| DceErr::closed0("ttl was not initialized yet."));
                #[cfg(not(feature = "async"))]
                return self.redis()?.ttl::<_, isize>(key).into_iter()
                    .find_map(|ttl| if ttl > 0 { Some(self.meta.ttl_seconds() - ttl as u32) } else { None })
                    .ok_or_else(|| DceErr::closed0("ttl was not initialized yet."));
            } )+
        }
        
        
        #[cfg_attr(feature = "async", async_trait)]
        impl<R, U> User<U> for RedisSession<R, U> where
            R: ConnectionLike + Send + Sync,
            U: Serialize + UidGetter + Send + Sync + for<'a> Deserialize<'a> + 'static,
        {
            fn user_meta(&self) -> &UserMeta<U> {
                &self.user_meta
            }
        
            fn user_meta_mut(&mut self) -> &mut UserMeta<U> {
                &mut self.user_meta
            }
            
            fn gen_user_key(user_prefix: &str, id: u64) -> String {
                format!("{}:{}", user_prefix, id)
            }
        
            #[cfg(feature = "async")]
            async fn mapping(&mut self) -> DceResult<bool> {
                // add the new sid into uid->sids mapping
                let user_key = self.user_key().await?;
                let id = self.id().to_string();
                self.redis()?.sadd(user_key.as_str(), id).await.map_err(DceErr::closed0)?;
                self.redis()?.expire(user_key.as_str(), MAPPING_TTL_SECONDS).await.map_err(DceErr::closed0)
            }
        
            #[cfg(not(feature = "async"))]
            fn mapping(&mut self) -> DceResult<bool> {
                // add the new sid into uid->sids mapping
                let user_key = self.user_key()?;
                let id = self.id().to_string();
                self.redis()?.sadd(user_key.as_str(), id).map_err(DceErr::closed0)?;
                self.redis()?.expire(user_key.as_str(), MAPPING_TTL_SECONDS).map_err(DceErr::closed0)
            }
        
            $($(#[$($meta),+])+
            $($async)? fn unmapping(&mut self) -> DceResult<bool> {
                // just need to try unmapping, because no user info if not logged there
                let sid = self.id().to_string();
                #[cfg(feature = "async")]
                if let Ok(user_key) = self.user_key().await {
                    return self.redis()?.srem(user_key, sid).await.map_err(DceErr::closed0);
                }
                #[cfg(not(feature = "async"))]
                if let Ok(user_key) = self.user_key() {
                    return self.redis()?.srem(user_key, sid).map_err(DceErr::closed0);
                }
                Ok(false)
            } )+
            
            $($(#[$($meta),+])+
            $($async)? fn sync(&mut self, user: &U) -> DceResult<bool> {
                let user_json = serde_json::to_string::<U>(user).map_err(DceErr::closed0)?;
                let user_field = self.user_meta().user_field().to_string();
                #[cfg(feature = "async")]
                for sid in self.sids(user.id()).await? {
                    let key = Self::gen_key(self.meta.sid_name(), sid.as_str());
                    self.redis()?.hset(key, user_field.as_str(), user_json.as_str()).await.map_err(DceErr::closed0)?;
                }
                #[cfg(not(feature = "async"))]
                for sid in self.sids(user.id())? {
                    let key = Self::gen_key(self.meta.sid_name(), sid.as_str());
                    self.redis()?.hset(key, user_field.as_str(), user_json.as_str()).map_err(DceErr::closed0)?;
                }
                Ok(true)
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn all_sid(&mut self, uid: u64) -> DceResult<HashSet<String>> {
                let user_key = Self::gen_user_key(self.user_meta.key_prefix(), uid);
                #[cfg(feature = "async")]
                return self.redis()?.smembers(user_key).await.map_err(DceErr::closed0);
                #[cfg(not(feature = "async"))]
                return self.redis()?.smembers(user_key).map_err(DceErr::closed0);
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn filter_sids(&mut self, uid: u64, sids: BTreeSet<String>) -> DceResult<BTreeSet<String>> {
                let user_key = Self::gen_user_key(self.user_meta.key_prefix(), uid);
                let mut filtered = BTreeSet::new();
                #[cfg(feature = "async")]
                for sid in sids {
                    let key = Self::gen_key(self.meta.sid_name(), sid.as_str());
                    if self.redis()?.exists(key).await.map_err(DceErr::closed0)? {
                        filtered.insert(sid);
                    } else {
                        let _ = self.redis()?.srem::<_, _, bool>(&user_key, sid).await.map_err(|e| warn!("{e}"));
                    }
                }
                #[cfg(not(feature = "async"))]
                for sid in sids {
                    let key = Self::gen_key(self.meta.sid_name(), sid.as_str());
                    if self.redis()?.exists(key).map_err(DceErr::closed0)? {
                        filtered.insert(sid);
                    } else {
                        let _ = self.redis()?.srem::<_, _, bool>(&user_key, sid).map_err(|e| warn!("{e}"));
                    }
                }
                Ok(filtered)
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_unmapping(&mut self) -> DceResult<bool> {                
                let keys = self.cloned.iter().find_map(|c| c.user_meta.user().map(|u| (Self::gen_user_key(c.user_meta.key_prefix(), u.id()), c.id().to_string())));
                if let Some((user_key, sid)) = keys {
                    #[cfg(feature = "async")]
                    return self.redis()?.srem(user_key, sid).await.map_err(DceErr::closed0);
                    #[cfg(not(feature = "async"))]
                    return self.redis()?.srem(user_key, sid).map_err(DceErr::closed0);
                }
                Ok(false)
            } )+
        }
    };
}
auto_async! {#[cfg(feature = "async")], async; #[cfg(not(feature = "async"))],}

#[cfg(feature = "connection")]
#[cfg_attr(feature = "async", async_trait)]
impl<R, U> Connection for RedisSession<R, U> where
    R: ConnectionLike + Send + Sync,
    U: Serialize + UidGetter + Send + Sync + for<'b> Deserialize<'b> + 'static,
{
    fn conn_meta(&self) -> &ConnectionMeta<Self> {
        &self.conn_meta
    }

    fn conn_meta_mut(&mut self) -> &mut ConnectionMeta<Self> {
        &mut self.conn_meta
    }

    #[cfg(feature = "async")]
    async fn unbinding(&mut self) -> DceResult<bool> {
        let (key, value) = (self.key(), self.conn_meta().server_field().to_string());
        return self.redis()?.hdel(key, value).await.map_err(DceErr::closed0);
    }

    #[cfg(not(feature = "async"))]
    fn unbinding(&mut self) -> DceResult<bool> {
        let (key, value) = (self.key(), self.conn_meta().server_field().to_string());
        return self.redis()?.hdel(key, value).map_err(DceErr::closed0);
    }
}
