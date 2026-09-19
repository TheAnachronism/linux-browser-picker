use std::rc::Rc;

use gtk::gdk;
use gtk::prelude::*;

use crate::list_order;

pub fn prepend_handle(header: &gtk::Box, name: &str) {
    let handle = gtk::Image::from_icon_name("list-drag-handle-symbolic");
    handle.set_pixel_size(16);
    handle.set_valign(gtk::Align::Center);
    handle.set_size_request(28, 28);
    handle.add_css_class("dim-label");
    handle.set_tooltip_text(Some(name));
    handle.update_property(&[
        gtk::accessible::Property::Label(name),
        gtk::accessible::Property::Description("Drag to reorder"),
    ]);
    header.prepend(&handle);
}

pub fn attach_row_drag(row: &gtk::ListBoxRow, on_drop: Rc<dyn Fn(usize, usize)>) {
    let drag = gtk::GestureDrag::new();
    drag.set_button(gdk::BUTTON_PRIMARY);
    let row_weak = row.downgrade();
    drag.connect_drag_update(|gesture, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    drag.connect_drag_end(move |gesture, _, y_offset| {
        let Some((_, start_y)) = gesture.start_point() else {
            return;
        };
        if y_offset.abs() < 12.0 {
            return;
        }
        let Some(row) = row_weak.upgrade() else {
            return;
        };
        let Some(parent) = row.parent() else {
            return;
        };
        let Ok(list) = parent.downcast::<gtk::ListBox>() else {
            return;
        };
        let Some(bounds) = row.compute_bounds(&list) else {
            return;
        };
        let y = f64::from(bounds.y()) + start_y + y_offset;
        on_drop(row.index() as usize, drop_insertion_index(&list, y));
    });
    row.add_controller(drag);
}

pub fn move_row_to<T>(
    list: &gtk::ListBox,
    items: &mut Vec<T>,
    row_of: impl Fn(&T) -> gtk::ListBoxRow,
    from: usize,
    to: usize,
) -> bool {
    if from >= items.len() {
        return false;
    }
    let row = row_of(&items[from]);
    if !list_order::move_item(items, from, to) {
        return false;
    }
    let dest = items
        .iter()
        .position(|item| row_of(item) == row)
        .expect("moved row remains in the draft");
    list.remove(&row);
    list.insert(&row, dest as i32);
    list.select_row(Some(&row));
    true
}

fn drop_insertion_index(list: &gtk::ListBox, y: f64) -> usize {
    if let Some(row) = list.row_at_y(y.round() as i32) {
        let index = row.index() as usize;
        let Some(bounds) = row.compute_bounds(list) else {
            return index;
        };
        if y > f64::from(bounds.y()) + f64::from(bounds.height()) / 2.0 {
            index + 1
        } else {
            index
        }
    } else {
        let mut count = 0;
        while list.row_at_index(count).is_some() {
            count += 1;
        }
        count as usize
    }
}
