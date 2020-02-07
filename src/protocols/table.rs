//! Spectrum implementation.
#![allow(dead_code)]

extern crate rand;
use crate::crypto::byte_utils::xor_bytes;
use crate::crypto::field::FieldElement;
use bytes::Bytes;

#[derive(Clone, PartialEq, Debug)]
pub struct ChannelKey {
    key: FieldElement,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ChannelTable {
    slot_size_in_bytes: usize,
    slots: Vec<Bytes>,
}

impl ChannelTable {
    pub fn new(num_slots: usize, slot_size_in_bytes: usize) -> ChannelTable {
        let zero = vec![0; slot_size_in_bytes];
        let slots: Vec<Bytes> = vec![Bytes::from(zero); num_slots];
        ChannelTable {
            slot_size_in_bytes,
            slots,
        }
    }
}

pub fn write_to_table(table: &mut ChannelTable, values: Vec<Bytes>) {
    for (slot, value) in table.slots.iter_mut().zip(values.iter()) {
        *slot = xor_bytes(&slot, &value);
    }
}

fn combine_tables(tables: Vec<ChannelTable>) -> Vec<Bytes> {
    assert!(tables.len() >= 2, "need at least two tables to combine!");
    let zero = vec![0; tables[0].slot_size_in_bytes];
    let mut slots: Vec<Bytes> = vec![Bytes::from(zero); tables[0].slots.len()];

    for table in tables.iter() {
        for (i, slot) in table.slots.iter().enumerate() {
            slots[i] = xor_bytes(&slots[i], &slot);
        }
    }

    slots
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const MAX_NUM_SLOTS: usize = 100;
    const MAX_NUM_TABLES: usize = 20;
    const MAX_SLOT_SIZE: usize = 100; // in bytes
    const MIN_SLOT_SIZE: usize = 1; // in bytes

    proptest! {

        #[test]
        fn test_write_to_slot_table(
            num_slots in (1..MAX_NUM_SLOTS),
            slot_size in (MIN_SLOT_SIZE..MAX_SLOT_SIZE),
            num_tables in (2..MAX_NUM_TABLES)
        ) {
            let all_one_slot = vec![1; slot_size];
            let all_zero_slot = vec![0; slot_size];

            let values_all_one = vec![Bytes::from(all_one_slot.clone()); num_slots];
            let values_all_zero = vec![Bytes::from(all_zero_slot.clone()); num_slots];

            let mut tables = vec![ChannelTable::new(num_slots, slot_size); num_tables];

            for table in tables.iter_mut() {
                write_to_table(table, values_all_one.clone())
            }

            // writing zero should have no effect
            for table in tables.iter_mut() {
                write_to_table(table, values_all_zero.clone())
            }

            // result of combining the tables
            let result = combine_tables(tables);

            for slot in result.iter() {
                if num_tables % 2 == 0 {
                                    // if even, all the writes will cancel out
                    assert_eq!(*slot, all_zero_slot);
                } else {
                                    // one of the writes will not cancel out when combined
                    assert_eq!(*slot, all_one_slot);
                }
            }

        }
    }
}
