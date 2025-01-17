use super::*;
use crate::kernel::abstract_domain::TryToBitvec;
use crossbeam_channel::Sender;

/// Compute various statistics about how exact memory accesses through `Load` and `Store` instructions are tracked.
/// Print the results as debug-log-messages.
pub fn compute_and_log_mem_access_stats(pointer_inference: &PointerInference) {
    MemAccessStats::compute_and_log(pointer_inference);
}

#[derive(Default)]
struct MemAccessStats {
    all_mem_ops: u64,
    contains_top_flag: u64,
    empty_errors: u64,
    is_only_top: u64,
    global_mem_access: u64,
    current_stack_access: u64,
    non_current_stack_access: u64,
    exact_target_with_exact_offset: u64,
    exact_target_with_top_offset: u64,
}

impl MemAccessStats {
    fn tracked_mem_ops(&self) -> u64 {
        self.all_mem_ops - self.is_only_top - self.contains_top_flag - self.empty_errors
    }

    fn ops_with_exact_target_known(&self) -> u64 {
        self.global_mem_access + self.current_stack_access + self.non_current_stack_access
    }

    fn print_general_stats(&self, log_collector: Sender<LogThreadMsg>) {
        let all_mem_ops = self.all_mem_ops as f64;
        let msg = format!(
            "Pointer Inference: {} memory operations.\n\
            \t{:.2}% tracked,\n\
            \t{:.2}% partially tracked,\n\
            \t{:.2}% untracked,\n\
            \t{:.2}% errors.",
            self.all_mem_ops,
            self.tracked_mem_ops() as f64 / all_mem_ops * 100.,
            self.contains_top_flag as f64 / all_mem_ops * 100.,
            self.is_only_top as f64 / all_mem_ops * 100.,
            self.empty_errors as f64 / all_mem_ops * 100.,
        );
        let log_msg = LogMessage::new_debug(msg);
        let _ = log_collector.send(LogThreadMsg::Log(log_msg));
    }

    fn print_tracked_mem_ops_stats(&self, log_collector: Sender<LogThreadMsg>) {
        let all_mem_ops = self.all_mem_ops as f64;
        let msg = format!(
            "Pointer Inference: {} ({:.2}%) memory operations with exactly known target. Of these are\n\
            \t{:.2}% global memory access,\n\
            \t{:.2}% current stack access,\n\
            \t{:.2}% other (heap or stack) access,\n\
            \t{:.2}% with constant offset,\n\
            \t{:.2}% with unknown offset.",
            self.ops_with_exact_target_known(),
            self.ops_with_exact_target_known() as f64 / all_mem_ops * 100.,
            self.global_mem_access as f64 / self.ops_with_exact_target_known() as f64 * 100.,
            self.current_stack_access as f64 / self.ops_with_exact_target_known() as f64 * 100.,
            self.non_current_stack_access as f64 / self.ops_with_exact_target_known() as f64 * 100.,
            self.exact_target_with_exact_offset as f64 / self.ops_with_exact_target_known() as f64 * 100.,
            self.exact_target_with_top_offset as f64 / self.ops_with_exact_target_known() as f64 * 100.,
        );
        let log_msg = LogMessage::new_debug(msg);
        let _ = log_collector.send(LogThreadMsg::Log(log_msg));
    }

    fn count_for_def(&mut self, state: &State, def: &Term<Def>) {
        use crate::kernel::abstract_domain::AbstractDomain;
        match &def.term {
            Def::Load { address, .. } | Def::Store { address, .. } => {
                self.all_mem_ops += 1;
                let address_val = state.eval(address);
                if address_val.is_empty() {
                    self.empty_errors += 1;
                }
                if address_val.is_top() {
                    self.is_only_top += 1;
                } else if address_val.contains_top() {
                    self.contains_top_flag += 1;
                }

                if let Some(offset) = address_val.get_if_absolute_value() {
                    self.global_mem_access += 1;
                    if offset.try_to_bitvec().is_ok() {
                        self.exact_target_with_exact_offset += 1;
                    } else if offset.is_top() {
                        self.exact_target_with_top_offset += 1;
                    }
                } else if let Some((id, offset)) = address_val.get_if_unique_target() {
                    if *id == state.stack_id {
                        self.current_stack_access += 1;
                    } else {
                        self.non_current_stack_access += 1;
                    }
                    if offset.try_to_bitvec().is_ok() {
                        self.exact_target_with_exact_offset += 1;
                    } else if offset.is_top() {
                        self.exact_target_with_top_offset += 1;
                    }
                }
            }
            Def::Assign { .. } => (),
        }
    }

    fn compute_and_log(pointer_inference: &PointerInference) {
        use crate::kernel::analysis::forward_interprocedural_fixpoint::Context as _;

        let mut stats = Self::default();
        let graph = pointer_inference.computation.get_graph();
        let context = pointer_inference.get_context();
        for (node_id, node) in graph.node_references() {
            if let Node::BlkStart(block, _sub) = node {
                if let Some(state) = pointer_inference.computation.get_node_value(node_id) {
                    let mut state = state.unwrap_value().clone();
                    for def in &block.term.defs {
                        stats.count_for_def(&state, def);
                        state = match context.update_def(&state, def) {
                            Some(new_state) => new_state,
                            None => break,
                        }
                    }
                }
            }
        }
        stats.print_general_stats(pointer_inference.log_collector.clone());
        stats.print_tracked_mem_ops_stats(pointer_inference.log_collector.clone());
    }
}
