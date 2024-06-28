use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use rand::random;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use dce_util::mixed::{DceErr, DceResult};
#[cfg(feature = "async")]
use async_trait::async_trait;

pub const DEFAULT_ID_NAME: &str = "dcesid";
pub const DEFAULT_TTL_MINUTES: u16 = 60;

#[derive(Clone)]
pub struct Meta {
    sid_name: &'static str,
    ttl_minutes: u16,
    create_stamp: u64,
    sid: String,
    touches: Option<bool>,
    #[cfg(feature = "test")]
    sid_pool: Vec<String>,
}

impl Meta {
    pub fn sid_name(&self) -> &str {
        self.sid_name
    }

    pub fn ttl_minutes(&self) -> u16 {
        self.ttl_minutes
    }

    pub fn ttl_seconds(&self) -> u32 {
        self.ttl_minutes as u32 * 60
    }

    pub fn create_stamp(&self) -> u64 {
        self.create_stamp
    }

    pub fn sid(&self) -> &str {
        self.sid.as_str()
    }

    pub fn config(&mut self, sid_name: Option<&'static str>, ttl_minutes: Option<u16>) {
        if let Some(sid_name) = sid_name { self.sid_name = sid_name; }
        if let Some(ttl_minutes) = ttl_minutes { self.ttl_minutes = ttl_minutes; }
    }

    pub fn renew(&mut self, sid: Option<String>) -> DceResult<()> {
        self.touches = None;
        Ok(if let Some(sid) = sid {
            (self.ttl_minutes, self.create_stamp) = Self::parse_sid(sid.as_str())?;
            self.sid = sid;
        } else {
            (self.sid, self.create_stamp) = Self::generate_id(self.ttl_minutes, #[cfg(feature = "test")] &mut self.sid_pool)?;
        })
    }

    pub fn new(ttl_minutes: u16) -> DceResult<Self> {
        #[cfg(feature = "test")] let mut sid_pool = vec![];
        let (sid, create_stamp) = Self::generate_id(ttl_minutes, #[cfg(feature = "test")] &mut sid_pool)?;
        Ok(Self {ttl_minutes, create_stamp, sid, sid_name: DEFAULT_ID_NAME, touches: None, #[cfg(feature = "test")] sid_pool })
    }

    pub fn new_with_sid(mut sid_pool: Vec<String>) -> DceResult<Self> {
        assert!(! sid_pool.is_empty());
        let sid = sid_pool.remove(0);
        let (ttl_minutes, create_stamp) = Self::parse_sid(&sid)?;
        Ok(Self {ttl_minutes, create_stamp, sid, sid_name: DEFAULT_ID_NAME, touches: None, #[cfg(feature = "test")] sid_pool })
    }

    fn parse_sid(sid: &str) -> DceResult<(u16, u64)> {
        const MIN_SID_LEN: usize = 76;
        if sid.len() < MIN_SID_LEN { return DceErr::closed0_wrap(format!(r#"invalid sid "{}", less then {} chars"#, sid, MIN_SID_LEN)); }
        let ttl_minutes = u16::from_str_radix(&sid[64..68], 16).map_err(DceErr::closed0)?;
        let create_stamp = u64::from_str_radix(&sid[68..], 16).map_err(DceErr::closed0)?;
        Ok((ttl_minutes, create_stamp))
    }

    #[allow(unused)]
    fn generate_id(ttl_minutes: u16, #[cfg(feature = "test")] sid_pool: &mut Vec<String>) -> DceResult<(String, u64)> {
        #[cfg(not(feature = "test"))] return Self::gen_id(ttl_minutes);
        #[cfg(feature = "test")] return {
            let sid = sid_pool.remove(0);
            Self::parse_sid(&sid).map(|(_, create_stamp)| (sid, create_stamp))
        };
    }

    pub fn gen_id(ttl_minutes: u16) -> DceResult<(String, u64)> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(DceErr::closed0)?;
        let now_secs = now.as_secs();
        let mut hasher = Sha256::new();
        hasher.update(format!("{}-{}", now.as_nanos(), random::<usize>()).as_bytes());
        Ok((format!("{:X}{:04X}{:X}", hasher.finalize(), ttl_minutes, now_secs), now_secs))
    }
}


macro_rules! auto_async {
    { $($(#[$($meta:meta),+])+, $($async: ident)?);+ } => {
        #[cfg_attr(feature = "async", async_trait)]
        pub trait Session: Sized {
            fn new(ttl_minutes: u16) -> DceResult<Self>;
            
            fn new_with_id(sid_pool: Vec<String>) -> DceResult<Self>;
        
            fn meta(&self) -> &Meta;
        
            fn meta_mut(&mut self) -> &mut Meta;
            
            fn id(&self) -> &str {
                self.meta().sid()
            }
        
            fn key(&self) -> String {
                Self::gen_key(self.meta().sid_name, self.id())
            }
        
            fn gen_key(sid_prefix: &str, id: &str) -> String;
        
            $($(#[$($meta),+])+
            $($async)? fn silent_set(&mut self, field: &str, value: &str) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn silent_get(&mut self, field: &str) -> DceResult<String>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn silent_del(&mut self, field: &str) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn destroy(&mut self) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn touch(&mut self) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn load(&mut self, data: HashMap<String, String>) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn raw(&mut self) -> DceResult<HashMap<String, String>>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn ttl_passed(&mut self) -> DceResult<u32>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn set<T: Serialize + Sync>(&mut self, field: &str, value: &T) -> DceResult<bool> {
                let value = serde_json::to_string::<T>(value).map_err(DceErr::closed0)?;
                #[cfg(feature = "async")]
                match self.silent_set(field, &value).await {
                    Ok(res) if res => self.try_touch().await,
                    result => result,
                }
                #[cfg(not(feature = "async"))]
                match self.silent_set(field, &value) {
                    Ok(res) if res => self.try_touch(),
                    result => result,
                }
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn get<T: for<'a> Deserialize<'a> + Send>(&mut self, field: &str) -> DceResult<T> {
                #[cfg(feature = "async")]
                let value = self.silent_get(field).await;
                #[cfg(not(feature = "async"))]
                let value = self.silent_get(field);
                if value.is_ok() {
                    #[cfg(feature = "async")]
                    self.try_touch().await?;
                    #[cfg(not(feature = "async"))]
                    self.try_touch()?;
                }
                serde_json::from_str(&value?).map_err(DceErr::closed0)
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn del(&mut self, field: &str) -> DceResult<bool> {
                #[cfg(feature = "async")]
                match self.silent_del(field).await {
                    Ok(res) if res => self.try_touch().await,
                    result => result,
                }
                #[cfg(not(feature = "async"))]
                match self.silent_del(field) {
                    Ok(res) if res => self.try_touch(),
                    result => result,
                }
            } )+
        
            $($(#[$($meta),+])+
            $($async)? fn try_touch(&mut self) -> DceResult<bool> {
                if self.meta().touches == None {
                    #[cfg(feature = "async")]
                    let touches = self.touch().await?;
                    #[cfg(not(feature = "async"))]
                    let touches = self.touch()?;
                    self.meta_mut().touches = Some(touches);
                }
                Ok(matches!(self.meta().touches, Some(true)))
            } )+
            
            $($(#[$($meta),+])+
            $($async)? fn renew(&mut self, filters: HashMap<String, Option<String>>) -> DceResult<bool> where Self: Send {
                #[cfg(feature = "async")]
                return renew(self, filters).await;
                #[cfg(not(feature = "async"))]
                return renew(self, filters);
            } )+
        
            fn clone_with_id(&mut self, id: String) -> DceResult<Self> where Self: Clone {
                clone_with_id(self, id)
            }

            fn cloned_mut(&mut self) -> &mut Option<Box<Self>>;
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_silent_set(&mut self, field: &str, value: &str) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_destroy(&mut self) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_touch(&mut self) -> DceResult<bool>; )+
        
            $($(#[$($meta),+])+
            $($async)? fn cloned_ttl_passed(&mut self) -> DceResult<u32>; )+
        }
        
        pub fn clone_with_id<S: Session + Clone>(session: &mut S, id: String) -> DceResult<S> {
            let mut cloned = session.clone();
            cloned.meta_mut().renew(Some(id))?;
            Ok(cloned)
        }
        
        $($(#[$($meta),+])+
        pub $($async)? fn renew<S: Session + Send>(session: &mut S, filters: HashMap<String, Option<String>>) -> DceResult<bool> {
            #[cfg(feature = "async")]
            let mut raw = session.raw().await?;
            #[cfg(not(feature = "async"))]
            let mut raw = session.raw()?;
            for (k, v) in filters {
                if let Some(v) = v {
                    let _ = raw.insert(k, v);
                } else {
                    let _ = raw.remove(&k);
                }
            }
            session.meta_mut().renew(None)?;
            if raw.is_empty() {
                return Ok(true);
            }
            #[cfg(feature = "async")]
            session.load(raw).await?;
            #[cfg(not(feature = "async"))]
            session.load(raw)?;
            #[cfg(feature = "async")]
            return session.try_touch().await;
            #[cfg(not(feature = "async"))]
            return session.try_touch();
        } )+
    };
}

auto_async!{ #[cfg(feature = "async")], async; #[cfg(not(feature = "async"))], }