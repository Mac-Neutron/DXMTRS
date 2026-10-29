use super::{extn_op_is_known, fill_sli};
use crate::nv::{EXTN_OP_FP16_ATOMIC, EXTN_OP_SHFL, EXTN_OP_UINT64_ATOMIC, SliStateV1};

#[test]
fn the_three_probed_opcodes_are_known_and_the_rest_are_not() {
    for op in [EXTN_OP_UINT64_ATOMIC, EXTN_OP_SHFL, EXTN_OP_FP16_ATOMIC] {
        assert!(extn_op_is_known(op));
    }
    assert!(!extn_op_is_known(0));
    assert!(!extn_op_is_known(99));
}

#[test]
fn sli_is_one_afr_group_and_leaves_the_version() {
    let mut state = SliStateV1 {
        version: 7,
        max_num_afr_groups: 9,
        num_afr_groups: 9,
        current_afr_index: 9,
        next_frame_afr_index: 9,
        previous_frame_afr_index: 9,
        is_cur_afr_group_new: 9,
    };
    fill_sli(&mut state);
    assert_eq!(
        [state.version, state.max_num_afr_groups, state.num_afr_groups, state.current_afr_index],
        [7, 1, 1, 0]
    );
    assert_eq!([state.next_frame_afr_index, state.previous_frame_afr_index, state.is_cur_afr_group_new], [0; 3]);
}
