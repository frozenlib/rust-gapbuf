#[macro_use]
extern crate proptest;

extern crate gapbuf;

use gapbuf::GapBuffer;

use proptest::prelude::*;

#[derive(Clone, Copy, Debug)]
enum VecAction {
    Insert(usize),
    Remove(usize),
    SetGap(usize),
}

fn vec_actions_strategy() -> impl Strategy<Value = Vec<VecAction>> {
    use self::VecAction::*;
    let idx = 0..100usize;
    let action = prop_oneof![
        idx.clone().prop_map(Insert),
        idx.clone().prop_map(Remove),
        idx.clone().prop_map(SetGap),
    ];
    proptest::collection::vec(action, 0..100)
}

proptest!{

    #[test]
    fn prop_insert_remove(actions in vec_actions_strategy()) {
        use self::VecAction::*;

        let mut e = Vec::new();
        let mut a = GapBuffer::new();

        let mut value = 0;
        for action in actions {
            match action {
                Insert(idx) => {
                    value += 1;
                    let idx = idx % (e.len() + 1);
                    e.insert(idx, value);
                    a.insert(idx, value);
                }
                Remove(idx) => {
                    if !e.is_empty() {
                        let idx = idx % e.len();
                        e.remove(idx);
                        a.remove(idx);
                    }
                }
                SetGap(idx) => {
                    let idx = idx % (e.len() + 1);
                    a.set_gap(idx);
                }
            }
            assert_eq!(a, e);
        }
    }
}