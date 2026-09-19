/// Insertion index after moving `from` to `to` in a list of `len`.
/// `to` is the drop index before the item is removed (0..=len).
pub fn insertion_after_move(len: usize, from: usize, to: usize) -> Option<usize> {
    if from >= len {
        return None;
    }
    let to = to.min(len);
    if from == to || from + 1 == to {
        return None;
    }
    Some(if to > from { to - 1 } else { to })
}

pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    let Some(dest) = insertion_after_move(items.len(), from, to) else {
        return false;
    };
    let item = items.remove(from);
    items.insert(dest, item);
    true
}

pub fn neighbor_insertion(len: usize, index: usize, direction: isize) -> Option<(usize, usize)> {
    if index >= len {
        return None;
    }
    let next = index as isize + direction;
    if next < 0 || next >= len as isize {
        return None;
    }
    let to = if direction > 0 {
        next as usize + 1
    } else {
        next as usize
    };
    Some((index, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbor_move_down_inserts_after_the_next_item() {
        assert_eq!(neighbor_insertion(3, 0, 1), Some((0, 2)));
        let mut items = vec!["alpha", "controlled", "unavailable"];
        assert!(move_item(&mut items, 0, 2));
        assert_eq!(items, ["controlled", "alpha", "unavailable"]);
    }

    #[test]
    fn neighbor_move_up_inserts_at_the_previous_item() {
        assert_eq!(neighbor_insertion(3, 1, -1), Some((1, 0)));
        let mut items = vec!["controlled", "alpha", "unavailable"];
        assert!(move_item(&mut items, 1, 0));
        assert_eq!(items, ["alpha", "controlled", "unavailable"]);
    }

    #[test]
    fn dropping_past_the_last_item_moves_it_to_the_end() {
        let mut items = vec!["alpha", "controlled", "unavailable"];
        assert_eq!(insertion_after_move(3, 0, 3), Some(2));
        assert!(move_item(&mut items, 0, 3));
        assert_eq!(items, ["controlled", "unavailable", "alpha"]);
    }

    #[test]
    fn dropping_on_the_same_item_or_past_the_end_of_itself_is_a_no_op() {
        let original = vec!["alpha", "controlled"];
        let mut items = original.clone();
        assert!(insertion_after_move(2, 0, 0).is_none());
        assert!(insertion_after_move(2, 0, 1).is_none());
        assert!(!move_item(&mut items, 0, 0));
        assert!(!move_item(&mut items, 0, 1));
        assert_eq!(items, original);
        assert!(neighbor_insertion(1, 0, 1).is_none());
        assert!(neighbor_insertion(2, 0, -1).is_none());
    }
}
