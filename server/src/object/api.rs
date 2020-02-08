use crate::object::actor::ObjectActor;
use crate::world::{Id, World};
use rlua;
use std::cell::RefCell;

pub struct ObjectApiExecutionState<'a> {
  actor: RefCell<&'a mut ObjectActor>,
}

impl<'a> ObjectApiExecutionState<'a> {
  fn new(actor: &mut ObjectActor) -> ObjectApiExecutionState {
    ObjectApiExecutionState {
      actor: RefCell::new(actor),
    }
  }

  fn with_state<T, F>(body: F) -> T
  where
    F: FnOnce(&ObjectApiExecutionState) -> T,
  {
    EXECUTION_STATE.with(|s| body(s))
  }

  fn with_actor<T, F>(body: F) -> T
  where
    F: FnOnce(&ObjectActor) -> T,
  {
    Self::with_state(|s| body(&s.actor.borrow()))
  }

  fn with_world<T, F>(body: F) -> T
  where
    F: FnOnce(&World) -> T,
  {
    Self::with_state(|s| s.actor.borrow().world.read(|w| body(w)))
  }

  fn get_id() -> Id {
    Self::with_actor(|a| a.id)
  }
}

scoped_thread_local! {static EXECUTION_STATE: ObjectApiExecutionState}

// API

mod api {
  use crate::chat::{ChatRowContent, ToClientMessage};
  use crate::lua::*;
  use crate::object::actor::ObjectMessage;
  use crate::object::api::ObjectApiExecutionState as S;
  use crate::world::Id;
  pub fn get_children(_lua_ctx: rlua::Context, object_id: Id) -> rlua::Result<Vec<Id>> {
    Ok(S::with_world(|w| {
      w.children(object_id).collect::<Vec<Id>>()
    }))
  }

  pub fn send(
    _lua_ctx: rlua::Context,
    (object_id, name, payload): (Id, String, SerializableValue),
  ) -> rlua::Result<()> {
    Ok(S::with_world(|w| {
      w.send_message(
        object_id,
        ObjectMessage {
          immediate_sender: S::get_id(),
          name: name,
          payload: payload,
        },
      )
    }))
  }

  pub fn tell(_lua_ctx: rlua::Context, message: String) -> rlua::Result<()> {
    Ok(S::with_world(|w| {
      w.send_client_message(
        S::get_id(),
        ToClientMessage::Tell {
          content: ChatRowContent::new(&message),
        },
      )
    }))
  }

  pub fn get_name(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<String> {
    Ok(S::with_world(|w| w.username(id)))
  }

  pub fn get_kind(_lua_ctx: rlua::Context, id: Id) -> rlua::Result<String> {
    Ok(S::with_world(|w| w.kind(id).0))
  }

  // orisa.set(
  //   "set_state",
  //   scope
  //     .create_function_mut(
  //       |_lua_ctx, (object_id, key, value): (Id, String, SerializableValue)| {
  //         if object_id != self.id {
  //           // Someday we might relax this given capabilities and probably containment (for concurrency)
  //           // Err("Can only set your own properties.")
  //           Ok(())
  //         } else {
  //           self.state.persistent_state.insert(key, value);
  //           Ok(())
  //         }
  //       },
  //     )
  //     .unwrap(),
  // );
}

pub fn register_api(lua_ctx: rlua::Context) -> rlua::Result<()> {
  let globals = lua_ctx.globals();
  let orisa = lua_ctx.create_table()?;

  orisa.set("get_children", lua_ctx.create_function(api::get_children)?)?;
  orisa.set("send", lua_ctx.create_function(api::send)?)?;
  orisa.set("tell", lua_ctx.create_function(api::tell)?)?;
  orisa.set("get_name", lua_ctx.create_function(api::get_name)?)?;
  orisa.set("get_kind", lua_ctx.create_function(api::get_kind)?)?;

  globals.set("orisa", orisa)?;
  Ok(())
}

pub fn with_api<'a, F, T>(actor: &mut ObjectActor, body: F) -> T
where
  F: FnOnce(rlua::Context) -> T,
{
  let state = ObjectApiExecutionState::new(actor);

  // This is a gross hack but is safe since the scoped thread local ensures
  // this value only exists as long as this block.
  EXECUTION_STATE.set(unsafe { make_static(&state) }, || {
    ObjectApiExecutionState::with_actor(|actor| actor.lua_state.context(|lua_ctx| body(lua_ctx)))
  })
}

unsafe fn make_static<'a>(
  p: &'a ObjectApiExecutionState<'a>,
) -> &'static ObjectApiExecutionState<'static> {
  use std::mem;
  mem::transmute(p)
}