//! The list grammar Filter and Layers share (README, Lists): selection
//! and order over stable IDs, listed bottom first here (the tag shows the
//! top of the stack first).
//!
//! What delete, mute and hide mean is the mode's; this only keeps the
//! selection sensible.

/// `[` / `]`: the item below (`-1`) or above (`+1`) the selected one,
/// stopping at the ends.
pub fn step<T: Copy + PartialEq>(ids: &[T], selected: Option<T>, delta: i32) -> Option<T> {
    let i = ids.iter().position(|&x| Some(x) == selected)?;
    let j = (i as i64 + i64::from(delta)).clamp(0, ids.len() as i64 - 1);
    Some(ids[j as usize])
}

/// `<` / `>`: the neighbour the selected item swaps with to move down
/// (`-1`) or up (`+1`), if there is one.
pub fn swap_with<T: Copy + PartialEq>(ids: &[T], selected: Option<T>, delta: i32) -> Option<T> {
    let i = ids.iter().position(|&x| Some(x) == selected)?;
    let j = i as i64 + i64::from(delta);
    (0..ids.len() as i64).contains(&j).then(|| ids[j as usize])
}

/// What's selected after deleting the item at `index` (of `before`, the
/// list as it was): the next surviving item, else the previous one.
///
/// PROVISIONAL (rust/PROGRESS.md, open question 1): "next" is taken in
/// display order, top first, so it's the item below in the stack; with
/// none below, the one above.
pub fn after_delete<T: Copy>(before: &[T], index: usize) -> Option<T> {
    if index > 0 {
        before.get(index - 1).copied()
    } else {
        before.get(1).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_stops_at_the_ends() {
        let ids = [1, 2, 3];
        assert_eq!(step(&ids, Some(2), -1), Some(1));
        assert_eq!(step(&ids, Some(1), -1), Some(1));
        assert_eq!(step(&ids, Some(3), 1), Some(3));
        assert_eq!(step(&ids, None, 1), None);
    }

    #[test]
    fn swap_only_with_a_neighbour() {
        let ids = [1, 2, 3];
        assert_eq!(swap_with(&ids, Some(2), 1), Some(3));
        assert_eq!(swap_with(&ids, Some(3), 1), None);
        assert_eq!(swap_with(&ids, Some(1), -1), None);
    }

    #[test]
    fn after_a_delete_the_next_row_down_else_the_one_up() {
        let ids = [1, 2, 3]; // shown 3, 2, 1
        assert_eq!(after_delete(&ids, 2), Some(2)); // the top: the row under it
        assert_eq!(after_delete(&ids, 1), Some(1));
        assert_eq!(after_delete(&ids, 0), Some(2)); // the bottom: the row over it
        assert_eq!(after_delete(&[7], 0), None);
    }
}
