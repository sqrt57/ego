use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::arena::ObjectId;

pub type Env = Rc<RefCell<HashMap<String, ObjectId>>>;

pub fn env_new() -> Env {
    Rc::new(RefCell::new(HashMap::new()))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActivationId(pub u64);
