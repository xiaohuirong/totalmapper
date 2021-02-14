
// vim: shiftwidth=2

use crate::keys::{Layout, Mapping, KeyCode, Pressed, Released, Event};

use std::collections::{HashMap, HashSet};
use std::cmp::Ordering;
use std::iter::FromIterator;

fn final_key(trigger: &Vec<KeyCode>) -> KeyCode {
  return trigger[trigger.len() - 1];
}

fn is_supported(trigger: &Vec<KeyCode>, pressed_keys: &Vec<KeyCode>, new_key: &KeyCode) -> bool {
  for k in trigger {
    if !(pressed_keys.contains(&k) || k == new_key) {
      return false;
    }
  }
  return true;
}

fn fails_when_released(trigger: &Vec<KeyCode>, key: &KeyCode) -> bool {
  for k in trigger {
    if k == key {
      return true;
    }
  }
  return false;
}

#[derive(Debug, Clone)]
struct ActiveMapping {
  from: Vec<KeyCode>,
  to: Vec<KeyCode>
}

#[derive(Debug)]
struct State {
  input_pressed_keys: Vec<KeyCode>,
  active_mappings: Vec<ActiveMapping>,
  pass_through_keys: Vec<KeyCode>,
  mapped_output_keys: Vec<KeyCode>,
}

fn init_state() -> State {
  return State {
    input_pressed_keys: Vec::new(),
    active_mappings: Vec::new(),
    pass_through_keys: Vec::new(),
    mapped_output_keys: Vec::new(),
  };
}

struct SeqMapping {
  from: Vec<KeyCode>,
  to: Vec<Vec<KeyCode>>
}

struct HashedLayout {
  mappings: HashMap<KeyCode, Vec<SeqMapping>>,
  no_repeat_keys: HashSet<KeyCode>
}

fn to_seq_mapping(m: &Mapping) -> SeqMapping {
  let mut working_modifiers: Vec<KeyCode> = Vec::new();
  let mut working_to: Vec<Vec<KeyCode>> = Vec::new();
  
  if m.to.len() > 0 {
    for k in &m.to[0 .. m.to.len()-1] {
      if is_action_key(k) {
        let mut combined = working_modifiers.clone();
        combined.push(*k);
        working_to.push(combined);
      }
      else {
        working_modifiers.push(*k);
      }
    }
  }

  if m.to.len() > 0 {
    let k = &m.to[m.to.len() - 1];
    let mut combined = working_modifiers.clone();
    combined.push(*k);
    working_to.push(combined);
  }
  
  SeqMapping {
    from: m.from.clone(),
    to: working_to
  }
}

fn trigger_priority(t1: &Vec<KeyCode>, t2: &Vec<KeyCode>) -> Ordering {
  if t1.len() > t2.len() {
    return Ordering::Less;
  }
  else if t1.len() < t2.len() {
    return Ordering::Greater;
  }
  else {
    for i in (0 .. t1.len()).rev() {
      if t1[i] < t2[i] {
        return Ordering::Less;
      }
      else if t1[i] > t1[i] {
        return Ordering::Greater;
      }
    }
    return Ordering::Equal;
  }
}

fn mapping_priority(m1: &SeqMapping, m2: &SeqMapping) -> Ordering {
  return trigger_priority(&m1.from, &m2.from);
}

fn make_hashed_layout(layout: &Layout) -> HashedLayout {
  let mut mappings: HashMap<KeyCode, Vec<SeqMapping>> = HashMap::new();

  for mapping in &layout.mappings {
    for i in 0 .. mapping.from.len() {
      for j in i+1 .. mapping.from.len() {
        if mapping.from[i] == mapping.from[j] {
          panic!("Duplicate key in from");
        }
      }
    }
    
    for i in 0 .. mapping.to.len() {
      for j in i+1 .. mapping.to.len() {
        if mapping.to[i] == mapping.to[j] {
          panic!("Duplicate key in to");
        }
      }
    }
  }
  
  for mapping in &layout.mappings {
    let last = final_key(&mapping.from);
    let seq_mapping = to_seq_mapping(&mapping);
    
    match mappings.get_mut(&last) {
      None => {
        mappings.insert(last, vec![seq_mapping]);
      },
      Some(existing) => {
        existing.push(seq_mapping);
        existing.sort_by(mapping_priority);
      }
    }
  }
  
  return HashedLayout {
    mappings: mappings,
    no_repeat_keys: HashSet::from_iter(layout.no_repeat_keys.iter().cloned())
  };
}

pub struct Mapper {
  layout: HashedLayout,
  state: State
}

impl Mapper {
  pub fn for_layout(layout: &Layout) -> Mapper {
    Mapper {
      layout: make_hashed_layout(layout),
      state: init_state()
    }
  }
  
  pub fn step(self: &mut Mapper, input: Event) -> Vec<Event> {
    let state = &mut self.state;

    match input {
      Pressed(k) => {
        if !state.input_pressed_keys.contains(&k) {
          return newly_press(self, k);
        }
        else {
          return vec![];
        }
      },
      Released(k) => {
        if state.input_pressed_keys.contains(&k) {
          return newly_release(self, k);
        }
        else {
          return vec![];
        }
      },
    }
  }
  
  pub fn release_all(self: &mut Mapper) -> Vec<Event> {
    let to_release = self.state.input_pressed_keys.clone();
    
    let mut res: Vec<Event> = Vec::new();
    
    for k in to_release {
      let mut chunk = self.step(Released(k));
      res.append(&mut chunk);
    }
    
    res
  }
}

fn is_action_key(k: &KeyCode) -> bool {
  use KeyCode::{LEFTSHIFT, RIGHTSHIFT, LEFTMETA, RIGHTMETA, LEFTCTRL, RIGHTCTRL};
  
  match k {
    LEFTSHIFT => false,
    RIGHTSHIFT => false,
    LEFTMETA => false,
    RIGHTMETA => false,
    LEFTCTRL => false,
    RIGHTCTRL => false,
    _ => true
  }
}

fn is_action_mapping(m: &ActiveMapping) -> bool {
  if m.to.len() == 0 {
    false
  }
  else {
    let last_key = &m.to[m.to.len() - 1];
    is_action_key(last_key)
  }
}

fn is_no_repeat_mapping(no_repeat_keys: &HashSet<KeyCode>, m: &ActiveMapping) -> bool {
  if m.to.len() == 0 {
    false
  }
  else {
    let last_key = &m.to[m.to.len() - 1];
    no_repeat_keys.contains(last_key)
  }
}

fn add_new_mapping(state: &mut State, m: &ActiveMapping) -> Vec<Event> {
  let mut res: Vec<Event> = Vec::new();
  
  let pass_through_keys = &mut state.pass_through_keys;
  let mapped_output_keys = &mut state.mapped_output_keys;
  
  pass_through_keys.retain(|&old_key| {
    if m.from.contains(&old_key) || m.to.contains(&old_key) {
      if !m.to.contains(&old_key) {
        res.push(Released(old_key));
        return false;
      }
      else {
        mapped_output_keys.push(old_key);
        return false;
      }
    }
    else {
      return true;
    }
  });
  
  let mut modifiers_to_release: Vec<KeyCode> = Vec::new();
  for exsting_mapping in &state.active_mappings {
    if is_action_mapping(exsting_mapping) {
      for mod_key in &exsting_mapping.to[0 .. exsting_mapping.to.len()-1] {
        if state.mapped_output_keys.contains(mod_key) {
          modifiers_to_release.push(*mod_key);
        }
      }
    }
  }
  
  for k in &modifiers_to_release {
    res.push(Released(*k));
  }
  state.mapped_output_keys.retain(|&k| {
    !modifiers_to_release.contains(&k)
  });
  
  for new_key in &m.to {
    if state.mapped_output_keys.contains(new_key) {
      res.push(Released(*new_key));
      res.push(Pressed(*new_key));
    }
    else {
      if state.pass_through_keys.contains(new_key) {
        res.push(Released(*new_key));
        res.push(Pressed(*new_key));
        state.pass_through_keys.retain(|k2| k2 != new_key);
        state.mapped_output_keys.push(*new_key);
      }
      else {
        res.push(Pressed(*new_key));
        state.mapped_output_keys.push(*new_key);
      }
    }
  }
  
  state.active_mappings.push(m.clone());
  
  return res;
}

fn apply_no_repeat_mapping(state: &mut State, m: &ActiveMapping) -> Vec<Event> {
  let mut res: Vec<Event> = Vec::new();
  
  let mut relevant_codes: Vec<KeyCode> = Vec::new();
  for k in &m.to {
    let already_pass_through = state.pass_through_keys.contains(k);
    let already_mapped = state.mapped_output_keys.contains(k);
    let already_down = already_pass_through || already_mapped;
    
    if !already_down {
      relevant_codes.push(*k);
    }
  }
  
  for k in &relevant_codes {
    res.push(Pressed(*k));
  }
  
  for k in relevant_codes.iter().rev() {
    res.push(Released(*k));
  }
  
  res
}

fn newly_press(mapper: &mut Mapper, k: KeyCode) -> Vec<Event> {
  let mappings = &mapper.layout.mappings;
  let state = &mut mapper.state;
  
  let mut res: Vec<Event> = Vec::new();
  
  let mut any_hit: bool = false;
  
  for mappings in mappings.get(&k) {
    for mapping in mappings {
      if is_supported(&mapping.from, &state.input_pressed_keys, &k) {
        for to in &mapping.to {
          let active_mapping = ActiveMapping {
            from: mapping.from.clone(),
            to: (*to).clone()
          };
          
          if is_no_repeat_mapping(&mapper.layout.no_repeat_keys, &active_mapping) {
            res.append(&mut apply_no_repeat_mapping(state, &active_mapping));
          }
          else {
            res.append(&mut add_new_mapping(state, &active_mapping));
          }
        }
        
        any_hit = true;
        break;
      }
    }
  }
  
  if !any_hit {
    for m in &state.active_mappings {
      if m.from.contains(&k) {
        any_hit = true;
        break;
      }
      else if m.to.contains(&k) {
        any_hit = true;
        break;
      }
    }
  }
  
  if !any_hit {
    if !state.pass_through_keys.contains(&k){
      if mapper.layout.no_repeat_keys.contains(&k) {
        res.push(Pressed(k));
        res.push(Released(k));
      }
      else {
        res.push(Pressed(k));
        state.pass_through_keys.push(k);
      }
    }
  }
  
  state.input_pressed_keys.push(k);
  
  return res;
}

fn remove_mapping(state: &mut State, i: usize, removed_key: KeyCode) -> Vec<Event> {
  let mut res: Vec<Event> = Vec::new();
  
  let active_mappings = &mut state.active_mappings;
  let input_pressed_keys = &state.input_pressed_keys;
  let pass_through_keys = &mut state.pass_through_keys;

  for mapped_output_i in (0 .. state.mapped_output_keys.len()).rev() {
    let k = state.mapped_output_keys[mapped_output_i];
    
    let mut still_used: bool = false;
    for j in 0 .. active_mappings.len() {
      if j != i {
        if active_mappings[j].to.contains(&k) {
          still_used = true;
          break;
        }
      }
    }

    if !still_used {
      if input_pressed_keys.contains(&k) && k != removed_key {
        let mut still_shadowed = false;
        for j in 0 .. active_mappings.len() {
          if j != i {
            if active_mappings[j].from.contains(&k) {
              still_shadowed = true;
              break;
            }
          }
        }
        if !still_shadowed {
          pass_through_keys.push(k);
        }
        else {
          res.push(Released(k));
        }
      }
      else {
        res.push(Released(k));
      }
    }
    
    if !still_used {
      state.mapped_output_keys.remove(mapped_output_i);
    }
  }
    
  active_mappings.remove(i);
  
  return res;
}

fn newly_release(mapper: &mut Mapper, k: KeyCode) -> Vec<Event> {
  let state = &mut mapper.state;
  
  let mut res: Vec<Event> = Vec::new();
  
  let mut i: isize = state.active_mappings.len() as isize - 1;
  while i >= 0 {
    if fails_when_released(&state.active_mappings[i as usize].from, &k) {
      res.append(&mut remove_mapping(state, i as usize, k));
    }
    i -= 1;
  }
  
  for i in (0 .. state.pass_through_keys.len()).rev() {
    if state.pass_through_keys[i] == k {
      res.push(Released(k));
      state.pass_through_keys.remove(i);
      break;
    }
  }
  
  state.input_pressed_keys.retain(|&old_key| {
    old_key != k
  });
  
  return res;
}

#[cfg(test)]
mod tests {
  use super::*;
  use KeyCode::*;
  
  #[test]
  fn test_most_basic() {
    let layout = Layout {
      mappings: vec![
        Mapping { from: vec![A], to: vec![B] },
      ],
      no_repeat_keys: Vec::new()
    };
    let mut mapper = Mapper::for_layout(&layout);
    assert_eq!(vec![Pressed(B)], mapper.step(Pressed(A)));
  }
  
  #[test]
  fn test_single_key_remap() {
    let layout = Layout {
      mappings: vec![
        Mapping { from: vec![A], to: vec![B] },
      ],
      no_repeat_keys: Vec::new()
    };
    let mut mapper = Mapper::for_layout(&layout);
    assert_eq!(vec![Pressed(B)], mapper.step(Pressed(A)));
    assert_eq!(vec![Released(B)], mapper.step(Released(A)));
    assert_eq!(vec![Pressed(C)], mapper.step(Pressed(C)));
    assert_eq!(vec![Released(C)], mapper.step(Released(C)));
    assert_eq!(vec![Pressed(LEFTSHIFT)], mapper.step(Pressed(LEFTSHIFT)));
    assert_eq!(vec![Pressed(B)], mapper.step(Pressed(A)));
  }
  
  #[test]
  fn test_multi_key_overlap() {
    let layout = Layout {
      mappings: vec![
        Mapping { from: vec![CAPSLOCK], to: vec![] },
        Mapping { from: vec![CAPSLOCK, M], to: vec![LEFTSHIFT, EQUAL] },
        Mapping { from: vec![CAPSLOCK, U], to: vec![EQUAL] },
      ],
      no_repeat_keys: Vec::new()
    };
    let mut mapper = Mapper::for_layout(&layout);
    let empty: Vec<Event> = Vec::new();
    
    assert_eq!(empty, mapper.step(Pressed(CAPSLOCK)));
    assert_eq!(vec![Pressed(LEFTSHIFT), Pressed(EQUAL)], mapper.step(Pressed(M)));
    assert_eq!(vec![Released(LEFTSHIFT), Released(EQUAL), Pressed(EQUAL)], mapper.step(Pressed(U)));
  }
  
  #[test]
  fn test_super_multi() {
    let layout = Layout {
      mappings: vec![
        Mapping { from: vec![CAPSLOCK], to: vec![] },
        Mapping { from: vec![TAB], to: vec![] },
        Mapping { from: vec![F], to: vec![U] },
        Mapping { from: vec![N], to: vec![B] },
        Mapping { from: vec![CAPSLOCK, M], to: vec![LEFTSHIFT, EQUAL] },
        Mapping { from: vec![CAPSLOCK, F], to: vec![EQUAL] },
        Mapping { from: vec![CAPSLOCK, N], to: vec![LEFTSHIFT, K1] },
        Mapping { from: vec![TAB, M], to: vec![PAGEDOWN] },
        Mapping { from: vec![TAB, N], to: vec![LEFTCTRL, LEFT] },
      ],
      no_repeat_keys: Vec::new()
    };
    let mut mapper = Mapper::for_layout(&layout);
    
    let empty: Vec<Event> = Vec::new();
    
    assert_eq!(vec![Pressed(LEFTSHIFT)], mapper.step(Pressed(LEFTSHIFT)));
    assert_eq!(empty, mapper.step(Pressed(TAB)));
    assert_eq!(vec![Pressed(LEFTCTRL), Pressed(LEFT)], mapper.step(Pressed(N)));
    assert_eq!(vec![Released(LEFT), Released(LEFTCTRL)], mapper.step(Released(N)));
    assert_eq!(empty, mapper.step(Released(TAB)));
    assert_eq!(vec![Pressed(M)], mapper.step(Pressed(M)));
    assert_eq!(vec![Released(M)], mapper.step(Released(M)));
    assert_eq!(vec![Released(LEFTSHIFT)], mapper.step(Released(LEFTSHIFT)));
    assert_eq!(empty, mapper.step(Pressed(CAPSLOCK)));
    assert_eq!(vec![Pressed(LEFTSHIFT), Pressed(EQUAL)], mapper.step(Pressed(M)));
  }
  
  #[test]
  fn test_no_repeat() {
    let layout = Layout {
      mappings: vec![
        Mapping { from: vec![A], to: vec![B] },
        Mapping { from: vec![C], to: vec![D] },
      ],
      no_repeat_keys: vec![B, E]
    };
    let mut mapper = Mapper::for_layout(&layout);
    
    let empty: Vec<Event> = Vec::new();
    
    assert_eq!(vec![Pressed(B), Released(B)], mapper.step(Pressed(A)));
    assert_eq!(empty, mapper.step(Released(A)));
    assert_eq!(vec![Pressed(E), Released(E)], mapper.step(Pressed(E)));
    assert_eq!(empty, mapper.step(Released(E)));
    assert_eq!(vec![Pressed(D)], mapper.step(Pressed(C)));
    assert_eq!(vec![Released(D)], mapper.step(Released(C)));
    assert_eq!(vec![Pressed(F)], mapper.step(Pressed(F)));
    assert_eq!(vec![Released(F)], mapper.step(Released(F)));
  }
  
  #[test]
  fn test_double_keys() {
    let layout = Layout {
      mappings: vec![
        Mapping { from: vec![A], to: vec![B, C] },
      ],
      no_repeat_keys: vec![C]
    };
    let mut mapper = Mapper::for_layout(&layout);
    
    assert_eq!(vec![Pressed(B), Pressed(C),  Released(C)], mapper.step(Pressed(A)));
  }
  
  #[test]
  fn test_double_keys_2() {
    let layout = Layout {
      mappings: vec![
        Mapping { from: vec![CAPSLOCK], to: vec![] },
        Mapping { from: vec![TAB], to: vec![] },
        
        Mapping { from: vec![J], to: vec![F16, H] },
        Mapping { from: vec![CAPSLOCK, J], to: vec![LEFTSHIFT, K0] },
        Mapping { from: vec![TAB, J], to: vec![LEFT] },
        
        Mapping { from: vec![N], to: vec![F17, B] },
        Mapping { from: vec![CAPSLOCK, N], to: vec![LEFTSHIFT, K1] },
        Mapping { from: vec![TAB, N], to: vec![LEFTCTRL, LEFT] },
      ],
      no_repeat_keys: vec![H, B, K1, K0]
    };
    let mut mapper = Mapper::for_layout(&layout);
    
    let empty: Vec<Event> = Vec::new();
    
    assert_eq!(vec![Pressed(LEFTSHIFT)], mapper.step(Pressed(LEFTSHIFT)));
    assert_eq!(empty, mapper.step(Pressed(TAB)));
    assert_eq!(vec![Pressed(LEFTCTRL), Pressed(LEFT)], mapper.step(Pressed(N)));
  }
}

