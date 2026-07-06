use std::rc::Rc;
use treewalk::arena::{Arena, NULL_ID};
use treewalk::env::{env_new, ActivationId};
use treewalk::gc::{collect, RootSet};
use treewalk::object::{BlockData, Object, ObjectKind, Slot, SlotKind};

fn plain() -> Object {
    Object::new(ObjectKind::Plain)
}

fn roots() -> RootSet {
    RootSet::new()
}

#[test]
fn live_objects_not_collected() {
    let mut arena = Arena::new();
    let mut r = roots();
    let id = arena.alloc(Object::new(ObjectKind::Integer(42)));
    r.lobby = id;
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 1);
    assert!(matches!(arena.get(id).kind, ObjectKind::Integer(42)));
}

#[test]
fn unreachable_objects_collected() {
    let mut arena = Arena::new();
    let r = roots();
    arena.alloc(plain());
    arena.alloc(plain());
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 0);
}

#[test]
fn slot_reference_keeps_target_live() {
    let mut arena = Arena::new();
    let mut r = roots();

    let target = arena.alloc(Object::new(ObjectKind::Integer(99)));
    let mut holder = Object::new(ObjectKind::Plain);
    holder.slots.push(Slot { name: "x".into(), kind: SlotKind::Data, value: target });
    let holder_id = arena.alloc(holder);

    r.lobby = holder_id;
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 2);
    assert!(matches!(arena.get(target).kind, ObjectKind::Integer(99)));
}

#[test]
fn parent_slot_keeps_target_live() {
    let mut arena = Arena::new();
    let mut r = roots();

    let parent = arena.alloc(Object::new(ObjectKind::Integer(1)));
    let mut child = plain();
    child.slots.push(Slot { name: "parent".into(), kind: SlotKind::Parent, value: parent });
    let child_id = arena.alloc(child);

    r.lobby = child_id;
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 2);
}

#[test]
fn cycle_both_collected_when_unreachable() {
    let mut arena = Arena::new();
    let r = roots();

    let id1 = arena.alloc(plain());
    let id2 = arena.alloc(plain());
    arena.get_mut(id1).slots.push(Slot { name: "other".into(), kind: SlotKind::Data, value: id2 });
    arena.get_mut(id2).slots.push(Slot { name: "other".into(), kind: SlotKind::Data, value: id1 });

    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 0);
}

#[test]
fn cycle_both_kept_when_one_rooted() {
    let mut arena = Arena::new();
    let mut r = roots();

    let id1 = arena.alloc(plain());
    let id2 = arena.alloc(plain());
    arena.get_mut(id1).slots.push(Slot { name: "other".into(), kind: SlotKind::Data, value: id2 });
    arena.get_mut(id2).slots.push(Slot { name: "other".into(), kind: SlotKind::Data, value: id1 });

    r.lobby = id1;
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 2);
}

#[test]
fn stack_roots_keep_objects_live() {
    let mut arena = Arena::new();
    let mut r = roots();

    let id = arena.alloc(Object::new(ObjectKind::Integer(5)));
    r.stack_roots.push(id);
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 1);
}

#[test]
fn all_permanent_root_fields_kept() {
    let mut arena = Arena::new();
    let mut r = roots();

    r.nil_id = arena.alloc(plain());
    r.true_id = arena.alloc(plain());
    r.false_id = arena.alloc(plain());
    r.lobby = arena.alloc(plain());

    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 4);
}

#[test]
fn block_captured_self_kept_live() {
    let mut arena = Arena::new();
    let mut r = roots();

    let self_obj = arena.alloc(Object::new(ObjectKind::Integer(42)));
    let block = Object::new(ObjectKind::Block(Box::new(BlockData {
        params: vec![],
        locals: vec![],
        body: Rc::new(vec![]),
        home_id: ActivationId(0),
        captured_self: self_obj,
        captured_resend: None,
        captures: env_new(),
    })));
    let block_id = arena.alloc(block);

    r.lobby = block_id;
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 2);
    assert!(matches!(arena.get(self_obj).kind, ObjectKind::Integer(42)));
}

#[test]
fn block_captured_resend_kept_live() {
    let mut arena = Arena::new();
    let mut r = roots();

    let resend_obj = arena.alloc(plain());
    let block = Object::new(ObjectKind::Block(Box::new(BlockData {
        params: vec![],
        locals: vec![],
        body: Rc::new(vec![]),
        home_id: ActivationId(0),
        captured_self: NULL_ID,
        captured_resend: Some(resend_obj),
        captures: env_new(),
    })));
    let block_id = arena.alloc(block);

    r.lobby = block_id;
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 2);
}

#[test]
fn block_env_captures_kept_live() {
    let mut arena = Arena::new();
    let mut r = roots();

    let val = arena.alloc(Object::new(ObjectKind::Integer(7)));
    let env = env_new();
    env.borrow_mut().insert("x".into(), val);

    let block = Object::new(ObjectKind::Block(Box::new(BlockData {
        params: vec![],
        locals: vec![],
        body: Rc::new(vec![]),
        home_id: ActivationId(0),
        captured_self: NULL_ID,
        captured_resend: None,
        captures: env,
    })));
    let block_id = arena.alloc(block);

    r.lobby = block_id;
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 2);
    assert!(matches!(arena.get(val).kind, ObjectKind::Integer(7)));
}

#[test]
fn block_env_unreachable_if_block_unreachable() {
    let mut arena = Arena::new();
    let r = roots();

    let val = arena.alloc(plain());
    let env = env_new();
    env.borrow_mut().insert("x".into(), val);

    let block = Object::new(ObjectKind::Block(Box::new(BlockData {
        params: vec![],
        locals: vec![],
        body: Rc::new(vec![]),
        home_id: ActivationId(0),
        captured_self: NULL_ID,
        captured_resend: None,
        captures: env,
    })));
    arena.alloc(block);

    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 0);
}

#[test]
fn repeated_collections_consistent() {
    let mut arena = Arena::new();
    let mut r = roots();

    let kept = arena.alloc(plain());
    r.lobby = kept;
    arena.alloc(plain()); // dead

    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 1);

    let also_kept = arena.alloc(Object::new(ObjectKind::Integer(3)));
    r.stack_roots.push(also_kept);
    arena.alloc(plain()); // dead again

    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 2);
}

#[test]
fn freed_slot_reused_after_collection() {
    let mut arena = Arena::new();
    let r = roots();

    let dead = arena.alloc(plain());
    collect(&mut arena, &r);
    assert_eq!(arena.live_count(), 0);

    let reused = arena.alloc(Object::new(ObjectKind::Integer(1)));
    assert_eq!(reused, dead); // slot was recycled
    assert_eq!(arena.live_count(), 1);
}
