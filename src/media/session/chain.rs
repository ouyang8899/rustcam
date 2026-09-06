use super::middleware::{Middleware, Next, MiddlewareResult, MediaContext};
use crate::media::errors::Result;

pub struct MiddlewareChain {
    middlewares: Vec<(String, Box<dyn Middleware>)>,
}

impl MiddlewareChain {
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }
    
    pub fn push<M: Middleware + 'static>(mut self, mw: M) -> Self {
        self.middlewares.push((mw.name().to_string(), Box::new(mw)));
        self
    }
    
    pub fn add(&mut self, name: &str, mw: Box<dyn Middleware>) {
        self.middlewares.push((name.to_string(), mw));
    }
    
    pub fn get(&mut self, name: &str) -> Option<&mut Box<dyn Middleware>> {
        self.middlewares.iter_mut().find(|(n, _)| n == name).map(|(_, mw)| mw)
    }
    
    pub fn remove(&mut self, name: &str) -> Option<Box<dyn Middleware>> {
        if let Some(pos) = self.middlewares.iter().position(|(n, _)| n == name) {
            Some(self.middlewares.remove(pos).1)
        } else {
            None
        }
    }
    
    pub fn names(&self) -> Vec<&str> {
        self.middlewares.iter().map(|(n, _)| n.as_str()).collect()
    }
    
    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }
    
    pub async fn execute(&self, ctx: &mut MediaContext) -> Result<()> {
        let next = Next::new(&self.middlewares);
        let result = next.run(ctx).await;
        
        match result {
            MiddlewareResult::Continue => Ok(()),
            MiddlewareResult::Stop => Ok(()),
            MiddlewareResult::Response(_) => Ok(()),
        }
    }
    
    pub fn iter(&self) -> impl Iterator<Item = (&str, &dyn Middleware)> {
        self.middlewares.iter().map(|(name, mw)| (name.as_str(), mw.as_ref()))
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}