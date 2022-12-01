use ethereum_types::U256;
use itertools::Itertools;
use plonky2::field::types::Field;

use crate::cpu::columns::CpuColumnsView;
use crate::cpu::kernel::aggregator::KERNEL;
use crate::cpu::membus::NUM_GP_CHANNELS;
use crate::cpu::simple_logic::eq_iszero::generate_pinv_diff;
use crate::memory::segments::Segment;
use crate::util::u256_saturating_cast_usize;
use crate::witness::errors::ProgramError;
use crate::witness::memory::{MemoryAddress, MemoryState};
use crate::witness::state::RegistersState;
use crate::witness::traces::Traces;
use crate::witness::util::{
    mem_read_code_with_log_and_fill, mem_read_gp_with_log_and_fill, mem_write_gp_log_and_fill,
    stack_pop_with_log_and_fill, stack_push_log_and_fill,
};
use crate::{arithmetic, logic};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Push(u8),
    Dup(u8),
    Swap(u8),
    Iszero,
    Not,
    Byte,
    Syscall(u8),
    Eq,
    BinaryLogic(logic::Op),
    BinaryArithmetic(arithmetic::BinaryOperator),
    TernaryArithmetic(arithmetic::TernaryOperator),
    KeccakGeneral,
    ProverInput,
    Pop,
    Jump,
    Jumpi,
    Pc,
    Gas,
    Jumpdest,
    GetContext,
    SetContext,
    ConsumeGas,
    ExitKernel,
    MloadGeneral,
    MstoreGeneral,
}

pub(crate) fn generate_binary_logic_op<F: Field>(
    op: logic::Op,
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(in0, log_in0), (in1, log_in1)] =
        stack_pop_with_log_and_fill::<2, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let operation = logic::Operation::new(op, in0, in1);
    let log_out =
        stack_push_log_and_fill(&mut registers_state, traces, &mut row, operation.result)?;

    traces.push_logic(operation);
    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_binary_arithmetic_op<F: Field>(
    operator: arithmetic::BinaryOperator,
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(input0, log_in0), (input1, log_in1)] =
        stack_pop_with_log_and_fill::<2, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let operation = arithmetic::Operation::binary(operator, input0, input1);
    let log_out =
        stack_push_log_and_fill(&mut registers_state, traces, &mut row, operation.result())?;

    traces.push_arithmetic(operation);
    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_ternary_arithmetic_op<F: Field>(
    operator: arithmetic::TernaryOperator,
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(input0, log_in0), (input1, log_in1), (input2, log_in2)] =
        stack_pop_with_log_and_fill::<3, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let operation = arithmetic::Operation::ternary(operator, input0, input1, input2);
    let log_out =
        stack_push_log_and_fill(&mut registers_state, traces, &mut row, operation.result())?;

    traces.push_arithmetic(operation);
    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_in2);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_prover_input<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    todo!()
}

pub(crate) fn generate_pop<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    todo!()
}

pub(crate) fn generate_jump<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(dst, log_in0)] =
        stack_pop_with_log_and_fill::<1, _>(&mut registers_state, memory_state, traces, &mut row)?;

    traces.push_memory(log_in0);
    traces.push_cpu(row);
    registers_state.program_counter = u256_saturating_cast_usize(dst);
    // TODO: Set other cols like input0_upper_sum_inv.
    Ok(registers_state)
}

pub(crate) fn generate_jumpi<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(dst, log_in0), (cond, log_in1)] =
        stack_pop_with_log_and_fill::<2, _>(&mut registers_state, memory_state, traces, &mut row)?;

    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_cpu(row);
    registers_state.program_counter = if cond.is_zero() {
        registers_state.program_counter + 1
    } else {
        u256_saturating_cast_usize(dst)
    };
    // TODO: Set other cols like input0_upper_sum_inv.
    Ok(registers_state)
}

pub(crate) fn generate_push<F: Field>(
    n: u8,
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let context = registers_state.effective_context();
    let num_bytes = n as usize + 1;
    let initial_offset = registers_state.program_counter + 1;
    let offsets = initial_offset..initial_offset + num_bytes;
    let mut addrs = offsets.map(|offset| MemoryAddress::new(context, Segment::Code, offset));

    // First read val without going through `mem_read_with_log` type methods, so we can pass it
    // to stack_push_log_and_fill.
    let bytes = (0..num_bytes)
        .map(|i| {
            memory_state
                .get(MemoryAddress::new(
                    context,
                    Segment::Code,
                    initial_offset + i,
                ))
                .as_u32() as u8
        })
        .collect_vec();

    let val = U256::from_big_endian(&bytes);
    let write = stack_push_log_and_fill(&mut registers_state, traces, &mut row, val)?;

    // In the first cycle, we read up to NUM_GP_CHANNELS - 1 bytes, leaving the last GP channel
    // to push the result.
    for (i, addr) in (&mut addrs).take(NUM_GP_CHANNELS - 1).enumerate() {
        let (_, read) = mem_read_gp_with_log_and_fill(i, addr, memory_state, traces, &mut row);
        traces.push_memory(read);
    }
    traces.push_memory(write);
    traces.push_cpu(row);

    // In any subsequent cycles, we read up to 1 + NUM_GP_CHANNELS bytes.
    for mut addrs_chunk in &addrs.chunks(1 + NUM_GP_CHANNELS) {
        let mut row = CpuColumnsView::default();
        // TODO: Set other row fields, like push=1?

        let first_addr = addrs_chunk.next().unwrap();
        let (_, first_read) =
            mem_read_code_with_log_and_fill(first_addr, memory_state, traces, &mut row);
        traces.push_memory(first_read);

        for (i, addr) in addrs_chunk.enumerate() {
            let (_, read) = mem_read_gp_with_log_and_fill(i, addr, memory_state, traces, &mut row);
            traces.push_memory(read);
        }

        traces.push_cpu(row);
    }

    Ok(registers_state)
}

pub(crate) fn generate_dup<F: Field>(
    n: u8,
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let other_addr_lo = registers_state
        .stack_len
        .checked_sub(1 + (n as usize))
        .ok_or(ProgramError::StackUnderflow)?;
    let other_addr = MemoryAddress::new(registers_state.context, Segment::Stack, other_addr_lo);

    let (val, log_in) =
        mem_read_gp_with_log_and_fill(0, other_addr, memory_state, traces, &mut row);
    let log_out = stack_push_log_and_fill(&mut registers_state, traces, &mut row, val)?;

    traces.push_memory(log_in);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_swap<F: Field>(
    n: u8,
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let other_addr_lo = registers_state
        .stack_len
        .checked_sub(2 + (n as usize))
        .ok_or(ProgramError::StackUnderflow)?;
    let other_addr = MemoryAddress::new(registers_state.context, Segment::Stack, other_addr_lo);

    let [(in0, log_in0)] =
        stack_pop_with_log_and_fill::<1, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let (in1, log_in1) =
        mem_read_gp_with_log_and_fill(1, other_addr, memory_state, traces, &mut row);
    let log_out0 =
        mem_write_gp_log_and_fill(NUM_GP_CHANNELS - 2, other_addr, traces, &mut row, in0);
    let log_out1 = stack_push_log_and_fill(&mut registers_state, traces, &mut row, in1)?;

    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_out0);
    traces.push_memory(log_out1);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_not<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(x, log_in)] =
        stack_pop_with_log_and_fill::<1, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let result = !x;
    let log_out = stack_push_log_and_fill(&mut registers_state, traces, &mut row, result)?;

    traces.push_memory(log_in);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_iszero<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(x, log_in)] =
        stack_pop_with_log_and_fill::<1, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let is_zero = x.is_zero();
    let result = {
        let t: u64 = is_zero.into();
        t.into()
    };
    let log_out = stack_push_log_and_fill(&mut registers_state, traces, &mut row, result)?;

    generate_pinv_diff(x, U256::zero(), &mut row);

    traces.push_memory(log_in);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_syscall<F: Field>(
    opcode: u8,
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let handler_jumptable_addr = KERNEL.global_labels["syscall_jumptable"] as usize;
    let handler_addr_addr = handler_jumptable_addr + (opcode as usize);
    let (handler_addr0, log_in0) = mem_read_gp_with_log_and_fill(
        0,
        MemoryAddress::new(0, Segment::Code, handler_addr_addr),
        memory_state,
        traces,
        &mut row,
    );
    let (handler_addr1, log_in1) = mem_read_gp_with_log_and_fill(
        1,
        MemoryAddress::new(0, Segment::Code, handler_addr_addr + 1),
        memory_state,
        traces,
        &mut row,
    );
    let (handler_addr2, log_in2) = mem_read_gp_with_log_and_fill(
        2,
        MemoryAddress::new(0, Segment::Code, handler_addr_addr + 2),
        memory_state,
        traces,
        &mut row,
    );

    let handler_addr = (handler_addr0 << 16) + (handler_addr1 << 8) + handler_addr2;
    let new_program_counter = handler_addr.as_usize();

    let syscall_info = U256::from(registers_state.program_counter)
        + (U256::from(u64::from(registers_state.is_kernel)) << 32);
    let log_out = stack_push_log_and_fill(&mut registers_state, traces, &mut row, syscall_info)?;

    registers_state.program_counter = new_program_counter;
    registers_state.is_kernel = true;

    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_in2);
    traces.push_memory(log_out);
    traces.push_cpu(row);

    Ok(registers_state)
}

pub(crate) fn generate_eq<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(in0, log_in0), (in1, log_in1)] =
        stack_pop_with_log_and_fill::<2, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let eq = in0 == in1;
    let result = U256::from(u64::from(eq));
    let log_out = stack_push_log_and_fill(&mut registers_state, traces, &mut row, result)?;

    generate_pinv_diff(in0, in1, &mut row);

    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_exit_kernel<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(kexit_info, log_in)] =
        stack_pop_with_log_and_fill::<1, _>(&mut registers_state, memory_state, traces, &mut row)?;
    let kexit_info_u64: [u64; 4] = kexit_info.0;
    let program_counter = kexit_info_u64[0] as usize;
    let is_kernel_mode_val = (kexit_info_u64[1] >> 32) as u32;
    assert!(is_kernel_mode_val == 0 || is_kernel_mode_val == 1);
    let is_kernel_mode = is_kernel_mode_val != 0;

    registers_state.program_counter = program_counter;
    registers_state.is_kernel = is_kernel_mode;

    traces.push_memory(log_in);
    traces.push_cpu(row);

    Ok(registers_state)
}

pub(crate) fn generate_mload_general<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(context, log_in0), (segment, log_in1), (virt, log_in2)] =
        stack_pop_with_log_and_fill::<3, _>(&mut registers_state, memory_state, traces, &mut row)?;

    // If virt won't fit in a usize, don't try to convert it, just return 0.
    let val = if virt > usize::MAX.into() {
        U256::zero()
    } else {
        memory_state.get(MemoryAddress {
            context: context.as_usize(),
            segment: segment.as_usize(),
            virt: virt.as_usize(),
        })
    };

    let log_out = stack_push_log_and_fill(&mut registers_state, traces, &mut row, val)?;

    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_in2);
    traces.push_memory(log_out);
    traces.push_cpu(row);
    Ok(registers_state)
}

pub(crate) fn generate_mstore_general<F: Field>(
    mut registers_state: RegistersState,
    memory_state: &MemoryState,
    traces: &mut Traces<F>,
    mut row: CpuColumnsView<F>,
) -> Result<RegistersState, ProgramError> {
    let [(context, log_in0), (segment, log_in1), (virt, log_in2), (val, log_in3)] =
        stack_pop_with_log_and_fill::<4, _>(&mut registers_state, memory_state, traces, &mut row)?;

    let address = MemoryAddress {
        context: context.as_usize(),
        segment: segment.as_usize(),
        virt: virt.as_usize(),
    };
    let log_write = mem_write_gp_log_and_fill(4, address, traces, &mut row, val);

    traces.push_memory(log_in0);
    traces.push_memory(log_in1);
    traces.push_memory(log_in2);
    traces.push_memory(log_in3);
    traces.push_memory(log_write);
    traces.push_cpu(row);
    Ok(registers_state)
}
