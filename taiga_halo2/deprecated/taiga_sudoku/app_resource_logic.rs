use ff::{Field, PrimeField};
use halo2_proofs::{
    circuit::{floor_planner, AssignedCell, Layouter, Value},
    plonk::{keygen_pk, keygen_vk, Advice, Circuit, Column, ConstraintSystem, Error},
};
use pasta_curves::pallas;
use rand::rngs::OsRng;
use rand::RngCore;
use taiga_halo2::{
    circuit::{
        gadgets::{
            assign_free_advice, assign_free_constant,
            mul::{MulChip, MulConfig, MulInstructions},
            poseidon_hash::poseidon_hash_gadget,
            sub::{SubChip, SubConfig, SubInstructions},
            target_resource_variable::{get_is_input_resource_flag, GetIsInputResourceFlagConfig},
            triple_mul::TripleMulConfig,
        },
        resource_circuit::ResourceConfig,
        resource_logic_circuit::{
            BasicResourceLogicVariables, InputResourceVariables, OutputResourceVariables,
            ResourceLogicVerifyingInfoTrait, ResourceLogicCircuit, ResourceLogicConfig,
            ResourceLogicInfo, ResourceLogicPublicInputs, ResourceLogicVerifyingInfo,
        },
    },
    constant::{NUM_RESOURCE, SETUP_PARAMS_MAP},
    resource::{Resource, RandomSeed},
    proof::Proof,
    utils::poseidon_hash,
    resource_logic_circuit_impl,
    resource_logic_vk::ResourceLogicVerifyingKey,
};

use crate::gadgets::{
    state_check::SudokuStateCheckConfig, state_update::StateUpdateConfig,
    value_check::ValueCheckConfig,
};
#[derive(Clone, Debug)]
pub struct SudokuState {
    pub state: [[u8; 9]; 9],
}

impl SudokuState {
    pub fn encode(&self) -> pallas::Base {
        // TODO: add the rho of resource to make the app_data_static unique.

        let sudoku = self.state.concat();
        let s1 = &sudoku[..sudoku.len() / 2]; // s1 contains 40 elements
        let s2 = &sudoku[sudoku.len() / 2..]; // s2 contains 41 elements
        let u: Vec<u8> = s1
            .iter()
            .zip(s2.iter()) // zip contains 40 elements
            .map(|(b1, b2)| {
                // Two entries of the sudoku can be seen as [b0,b1,b2,b3] and [c0,c1,c2,c3]
                // We store [b0,b1,b2,b3,c0,c1,c2,c3] here.
                assert!(b1 + 16 * b2 < 255);
                b1 + 16 * b2
            })
            .chain(s2.last().copied()) // there's 41st element in s2, so we add it here
            .collect();

        // fill u with zeros.
        // The length of u is 41 bytes, or 328 bits, since we are allocating 4 bits
        // per the first 40 integers and let the last sudoku digit takes an entire byte.
        // We still need to add 184 bits (i.e. 23 bytes) to reach 2*256=512 bits in total.
        // let u2 = [u, vec![0; 23]].concat(); // this is not working with all puzzles
        // For some reason, not _any_ byte array can be transformed into a 256-bit field element.
        // Preliminary investigation shows that `pallas::Base::from_repr` fails on a 32 byte array
        // if the first bit of every 8-byte (== u64) chunk is set to '1'. For now, we just add a zero
        // byte every 7 bytes, which is not ideal but works. Further investigation is needed.
        let mut u2 = [0u8; 64];
        let mut i = 0;
        let mut j = 0;
        while j != u.len() {
            if (i + 1) % 8 != 0 {
                u2[i] = u[j];
                j += 1;
            }
            i += 1;
        }
        let u_first: [u8; 32] = u2[0..32].try_into().unwrap();
        let u_last: [u8; 32] = u2[32..].try_into().unwrap();

        let x = pallas::Base::from_repr(u_first).unwrap();
        let y = pallas::Base::from_repr(u_last).unwrap();
        poseidon_hash(x, y)
    }
}

impl Default for SudokuState {
    fn default() -> Self {
        SudokuState {
            state: [
                [7, 0, 9, 5, 3, 8, 1, 2, 4],
                [2, 0, 3, 7, 1, 9, 6, 5, 8],
                [8, 0, 1, 4, 6, 2, 9, 7, 3],
                [4, 0, 6, 9, 7, 5, 3, 1, 2],
                [5, 0, 7, 6, 2, 1, 4, 8, 9],
                [1, 0, 2, 8, 4, 3, 7, 6, 5],
                [6, 0, 8, 3, 5, 4, 2, 9, 7],
                [9, 0, 4, 2, 8, 6, 5, 3, 1],
                [3, 0, 5, 1, 9, 7, 8, 4, 6],
            ],
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SudokuAppResourceLogicCircuit {
    owned_resource_id: pallas::Base,
    input_resources: [Resource; NUM_RESOURCE],
    output_resources: [Resource; NUM_RESOURCE],
    // Initial puzzle encoded in a single field
    encoded_init_state: pallas::Base,
    // If it is a init state, previous_state is equal to current_state
    previous_state: SudokuState,
    current_state: SudokuState,
}

#[derive(Clone, Debug)]
struct SudokuAppResourceLogicConfig {
    resource_config: ResourceConfig,
    advices: [Column<Advice>; 10],
    get_is_input_resource_flag_config: GetIsInputResourceFlagConfig,
    sudoku_state_check_config: SudokuStateCheckConfig,
    state_update_config: StateUpdateConfig,
    triple_mul_config: TripleMulConfig,
    value_check_config: ValueCheckConfig,
    sub_config: SubConfig,
    mul_config: MulConfig,
}

impl SudokuAppResourceLogicConfig {
    pub fn sub_chip(&self) -> SubChip<pallas::Base> {
        SubChip::construct(self.sub_config.clone(), ())
    }

    pub fn mul_chip(&self) -> MulChip<pallas::Base> {
        MulChip::construct(self.mul_config.clone())
    }
}

impl ResourceLogicConfig for SudokuAppResourceLogicConfig {
    fn get_resource_config(&self) -> ResourceConfig {
        self.resource_config.clone()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self {
        let resource_config = Self::configure_resource(meta);

        let advices = resource_config.advices;
        let sudoku_state_check_config = SudokuStateCheckConfig::configure(
            meta, advices[0], advices[1], advices[2], advices[3], advices[4], advices[5],
            advices[6], advices[7],
        );
        let state_update_config =
            StateUpdateConfig::configure(meta, advices[0], advices[1], advices[2]);
        let triple_mul_config = TripleMulConfig::configure(meta, advices[0..3].try_into().unwrap());
        let value_check_config =
            ValueCheckConfig::configure(meta, advices[0], advices[1], advices[2]);
        let sub_config = SubChip::configure(meta, [advices[0], advices[1]]);
        let mul_config = MulChip::configure(meta, [advices[0], advices[1]]);
        let get_is_input_resource_flag_config =
            GetIsInputResourceFlagConfig::configure(meta, advices[0], advices[1], advices[2]);
        Self {
            resource_config,
            advices,
            get_is_input_resource_flag_config,
            sudoku_state_check_config,
            state_update_config,
            triple_mul_config,
            value_check_config,
            sub_config,
            mul_config,
        }
    }
}

impl SudokuAppResourceLogicCircuit {
    // Copy from valid_puzzle/circuit.rs
    #[allow(clippy::too_many_arguments)]
    fn check_puzzle(
        mut layouter: impl Layouter<pallas::Base>,
        config: &SudokuAppResourceLogicConfig,
        // advice: Column<Advice>,
        state: &[AssignedCell<pallas::Base, pallas::Base>],
    ) -> Result<(), Error> {
        let non_zero_sudoku_cells: Vec<AssignedCell<pallas::Base, pallas::Base>> = state
            .iter()
            .enumerate()
            .map(|(i, x)| {
                // TODO: fix it, add constraints for non_zero_sudoku_cells assignment
                let ret = x.value().map(|x| {
                    if *x == pallas::Base::zero() {
                        pallas::Base::from_u128(10 + i as u128)
                    } else {
                        *x
                    }
                });
                assign_free_advice(layouter.namespace(|| "sudoku_cell"), config.advices[0], ret)
                    .unwrap()
            })
            .collect();

        // rows
        let rows: Vec<Vec<AssignedCell<pallas::Base, pallas::Base>>> = non_zero_sudoku_cells
            .chunks(9)
            .map(|row| row.to_vec())
            .collect();
        // cols
        let cols: Vec<Vec<AssignedCell<pallas::Base, pallas::Base>>> = (1..10)
            .map(|i| {
                let col: Vec<AssignedCell<pallas::Base, pallas::Base>> = non_zero_sudoku_cells
                    .chunks(9)
                    .map(|row| row[i - 1].clone())
                    .collect();
                col
            })
            .collect();
        // small squares
        let mut squares: Vec<Vec<AssignedCell<pallas::Base, pallas::Base>>> = vec![];
        for i in 1..4 {
            for j in 1..4 {
                let sub_lines = &rows[(i - 1) * 3..i * 3];

                let square: Vec<&[AssignedCell<pallas::Base, pallas::Base>]> = sub_lines
                    .iter()
                    .map(|line| &line[(j - 1) * 3..j * 3])
                    .collect();
                squares.push(square.concat());
            }
        }

        for perm in [rows, cols, squares].concat().iter() {
            let mut cell_lhs = assign_free_advice(
                layouter.namespace(|| "lhs init"),
                config.advices[0],
                Value::known(pallas::Base::one()),
            )
            .unwrap();
            for i in 0..9 {
                for j in (i + 1)..9 {
                    let diff = SubInstructions::sub(
                        &config.sub_chip(),
                        layouter.namespace(|| "diff"),
                        &perm[i],
                        &perm[j],
                    )
                    .unwrap();
                    cell_lhs = MulInstructions::mul(
                        &config.mul_chip(),
                        layouter.namespace(|| "lhs * diff"),
                        &cell_lhs,
                        &diff,
                    )
                    .unwrap();
                }
            }
            let cell_lhs_inv = assign_free_advice(
                layouter.namespace(|| "non-zero sudoku_cell"),
                config.advices[0],
                cell_lhs.value().map(|x| x.invert().unwrap()),
            )
            .unwrap();

            let cell_div = MulInstructions::mul(
                &config.mul_chip(),
                layouter.namespace(|| "lhs * 1/lhs"),
                &cell_lhs,
                &cell_lhs_inv,
            )
            .unwrap();

            let constant_one = assign_free_constant(
                layouter.namespace(|| "constant one"),
                config.advices[0],
                pallas::Base::one(),
            )?;

            layouter.assign_region(
                || "lhs * 1/lhs = 1",
                |mut region| region.constrain_equal(cell_div.cell(), constant_one.cell()),
            )?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn check_state(
        config: &SudokuStateCheckConfig,
        mut layouter: impl Layouter<pallas::Base>,
        is_input_resource: &AssignedCell<pallas::Base, pallas::Base>,
        init_state: &AssignedCell<pallas::Base, pallas::Base>,
        input_resource_pre_state: &AssignedCell<pallas::Base, pallas::Base>,
        output_resource_cur_state: &AssignedCell<pallas::Base, pallas::Base>,
        input_resource_app_data_static_encode: &AssignedCell<pallas::Base, pallas::Base>,
        input_resource: &InputResourceVariables,
        output_resource: &OutputResourceVariables,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "dealer intent check",
            |mut region| {
                config.assign_region(
                    is_input_resource,
                    init_state,
                    &input_resource.resource_variables.app_data_static,
                    input_resource_app_data_static_encode,
                    &input_resource.resource_variables.app_vk,
                    &output_resource.resource_variables.app_vk,
                    input_resource_pre_state,
                    output_resource_cur_state,
                    0,
                    &mut region,
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn check_solution(
        mut layouter: impl Layouter<pallas::Base>,
        state_update_config: &StateUpdateConfig,
        triple_mul_config: &TripleMulConfig,
        value_check_config: &ValueCheckConfig,
        is_input_resource: &AssignedCell<pallas::Base, pallas::Base>,
        pre_state: &[AssignedCell<pallas::Base, pallas::Base>],
        cur_state: &[AssignedCell<pallas::Base, pallas::Base>],
        input_resource: &InputResourceVariables,
        output_resource: &OutputResourceVariables,
    ) -> Result<(), Error> {
        // check state update: the cur_state is updated from pre_state
        pre_state
            .iter()
            .zip(cur_state.iter())
            .for_each(|(pre_state_cell, cur_state_cell)| {
                layouter
                    .assign_region(
                        || "state update check",
                        |mut region| {
                            state_update_config.assign_region(
                                is_input_resource,
                                pre_state_cell,
                                cur_state_cell,
                                0,
                                &mut region,
                            )
                        },
                    )
                    .unwrap();
            });

        // if cur_state is the final solution, check the output.quantity is zero else check the output.quantity is one
        // ret has 27 elements
        let ret: Vec<AssignedCell<pallas::Base, pallas::Base>> = cur_state
            .chunks(3)
            .map(|triple| {
                layouter
                    .assign_region(
                        || "triple mul",
                        |mut region| {
                            triple_mul_config.assign_region(
                                &triple[0],
                                &triple[1],
                                &triple[2],
                                0,
                                &mut region,
                            )
                        },
                    )
                    .unwrap()
            })
            .collect();
        // ret has 9 elements
        let ret: Vec<AssignedCell<pallas::Base, pallas::Base>> = ret
            .chunks(3)
            .map(|triple| {
                layouter
                    .assign_region(
                        || "triple mul",
                        |mut region| {
                            triple_mul_config.assign_region(
                                &triple[0],
                                &triple[1],
                                &triple[2],
                                0,
                                &mut region,
                            )
                        },
                    )
                    .unwrap()
            })
            .collect();
        // ret has 3 elements
        let ret: Vec<AssignedCell<pallas::Base, pallas::Base>> = ret
            .chunks(3)
            .map(|triple| {
                layouter
                    .assign_region(
                        || "triple mul",
                        |mut region| {
                            triple_mul_config.assign_region(
                                &triple[0],
                                &triple[1],
                                &triple[2],
                                0,
                                &mut region,
                            )
                        },
                    )
                    .unwrap()
            })
            .collect();
        let product = layouter.assign_region(
            || "triple mul",
            |mut region| triple_mul_config.assign_region(&ret[0], &ret[1], &ret[2], 0, &mut region),
        )?;

        layouter.assign_region(
            || "check quantity",
            |mut region| {
                value_check_config.assign_region(
                    is_input_resource,
                    &product,
                    &input_resource.resource_variables.quantity,
                    &output_resource.resource_variables.quantity,
                    0,
                    &mut region,
                )
            },
        )?;

        Ok(())
    }
}

impl ResourceLogicInfo for SudokuAppResourceLogicCircuit {
    fn get_input_resources(&self) -> &[Resource; NUM_RESOURCE] {
        &self.input_resources
    }

    fn get_output_resources(&self) -> &[Resource; NUM_RESOURCE] {
        &self.output_resources
    }

    fn get_public_inputs(&self, mut rng: impl RngCore) -> ResourceLogicPublicInputs {
        let mut public_inputs = self.get_mandatory_public_inputs();
        let padding = ResourceLogicPublicInputs::get_public_input_padding(
            public_inputs.len(),
            &RandomSeed::random(&mut rng),
        );
        public_inputs.extend(padding);
        public_inputs.into()
    }

    fn get_owned_resource_id(&self) -> pallas::Base {
        self.owned_resource_id
    }
}

impl ResourceLogicCircuit for SudokuAppResourceLogicCircuit {
    type ResourceLogicConfig = SudokuAppResourceLogicConfig;
    // Add custom constraints
    fn custom_constraints(
        &self,
        config: Self::ResourceLogicConfig,
        mut layouter: impl Layouter<pallas::Base>,
        basic_variables: BasicResourceLogicVariables,
    ) -> Result<(), Error> {
        let owned_resource_id = basic_variables.get_owned_resource_id();
        let is_input_resource = get_is_input_resource_flag(
            config.get_is_input_resource_flag_config,
            layouter.namespace(|| "get is_input_resource_flag"),
            &owned_resource_id,
            &basic_variables.get_input_resource_nfs(),
            &basic_variables.get_output_resource_cms(),
        )?;

        // witness the sudoku previous state
        let previous_sudoku_cells: Vec<AssignedCell<_, _>> = self
            .previous_state
            .state
            .concat()
            .iter()
            .map(|x| {
                assign_free_advice(
                    layouter.namespace(|| "sudoku_cell"),
                    config.advices[0],
                    Value::known(pallas::Base::from_u128(*x as u128)),
                )
                .unwrap()
            })
            .collect();

        // witness the sudoku current state
        let current_sudoku_cells: Vec<AssignedCell<_, _>> = self
            .current_state
            .state
            .concat()
            .iter()
            .map(|x| {
                assign_free_advice(
                    layouter.namespace(|| "sudoku_cell"),
                    config.advices[0],
                    Value::known(pallas::Base::from_u128(*x as u128)),
                )
                .unwrap()
            })
            .collect();

        // TODO: constrain the encoding of states instead of witnessing them.
        let encoded_previous_state = assign_free_advice(
            layouter.namespace(|| "witness encoded_previous_state"),
            config.advices[0],
            Value::known(self.previous_state.encode()),
        )?;

        let encoded_current_state = assign_free_advice(
            layouter.namespace(|| "witness encoded_current_state"),
            config.advices[0],
            Value::known(self.current_state.encode()),
        )?;

        // app_data_static = poseidon_hash(encoded_init_state || encoded_state)
        let encoded_init_state = assign_free_advice(
            layouter.namespace(|| "witness encoded_init_state"),
            config.advices[0],
            Value::known(self.encoded_init_state),
        )?;
        let input_resource_app_data_static_encode = poseidon_hash_gadget(
            config.get_resource_config().poseidon_config,
            layouter.namespace(|| "input resource app_data_static encoding"),
            [encoded_init_state.clone(), encoded_previous_state.clone()],
        )?;

        let output_resource_app_data_static_encode = poseidon_hash_gadget(
            config.get_resource_config().poseidon_config,
            layouter.namespace(|| "output resource app_data_static encoding"),
            [encoded_init_state.clone(), encoded_current_state.clone()],
        )?;

        layouter.assign_region(
            || "check output resource app_data_static encoding",
            |mut region| {
                region.constrain_equal(
                    output_resource_app_data_static_encode.cell(),
                    basic_variables.output_resource_variables[0]
                        .resource_variables
                        .app_data_static
                        .cell(),
                )
            },
        )?;

        Self::check_puzzle(
            layouter.namespace(|| "check puzzle"),
            &config,
            &current_sudoku_cells,
        )?;

        // check state
        Self::check_state(
            &config.sudoku_state_check_config,
            layouter.namespace(|| "check state"),
            &is_input_resource,
            &encoded_init_state,
            &encoded_previous_state,
            &encoded_current_state,
            &input_resource_app_data_static_encode,
            &basic_variables.input_resource_variables[0],
            &basic_variables.output_resource_variables[0],
        )?;

        // if it is an input resource, check that the cur_state is updated from pre_state
        // if encoded_current_state is the final solution, check the output.quantity is zero else check the output.quantity is one
        Self::check_solution(
            layouter.namespace(|| "check solution"),
            &config.state_update_config,
            &config.triple_mul_config,
            &config.value_check_config,
            &is_input_resource,
            &previous_sudoku_cells,
            &current_sudoku_cells,
            &basic_variables.input_resource_variables[0],
            &basic_variables.output_resource_variables[0],
        )?;

        Ok(())
    }
}

resource_logic_circuit_impl!(SudokuAppResourceLogicCircuit);

#[cfg(test)]
pub mod tests {
    use halo2_proofs::arithmetic::Field;
    use pasta_curves::pallas;
    use rand::{Rng, RngCore};
    use taiga_halo2::{
        resource::{Resource, ResourceKind, RandomSeed},
        nullifier::{Nullifier, NullifierKeyContainer},
    };

    pub fn random_input_resource<R: RngCore>(mut rng: R) -> Resource {
        let rho = Nullifier::from(pallas::Base::random(&mut rng));
        let nk = NullifierKeyContainer::from_key(pallas::Base::random(&mut rng));
        let kind = {
            let app_vk = pallas::Base::random(&mut rng);
            let app_data_static = pallas::Base::random(&mut rng);
            ResourceKind::new(app_vk, app_data_static)
        };
        let app_data_dynamic = pallas::Base::random(&mut rng);
        let quantity: u64 = rng.gen();
        let rseed = RandomSeed::random(&mut rng);
        Resource {
            kind,
            app_data_dynamic,
            quantity,
            nk_container: nk,
            is_merkle_checked: true,
            psi: rseed.get_psi(&rho),
            rcm: rseed.get_rcm(&rho),
            rho,
        }
    }

    pub fn random_output_resource<R: RngCore>(mut rng: R, rho: Nullifier) -> Resource {
        let nk_com = NullifierKeyContainer::from_commitment(pallas::Base::random(&mut rng));
        let kind = {
            let app_vk = pallas::Base::random(&mut rng);
            let app_data_static = pallas::Base::random(&mut rng);
            ResourceKind::new(app_vk, app_data_static)
        };
        let app_data_dynamic = pallas::Base::random(&mut rng);
        let quantity: u64 = rng.gen();
        let rseed = RandomSeed::random(&mut rng);
        Resource {
            kind,
            app_data_dynamic,
            quantity,
            nk_container: nk_com,
            is_merkle_checked: true,
            psi: rseed.get_psi(&rho),
            rcm: rseed.get_rcm(&rho),
            rho,
        }
    }
}

#[test]
fn test_halo2_sudoku_app_resource_logic_circuit_init() {
    use crate::app_resource_logic::tests::{random_input_resource, random_output_resource};
    use halo2_proofs::dev::MockProver;
    use rand::rngs::OsRng;

    let mut rng = OsRng;
    let circuit = {
        let input_resources = [(); NUM_RESOURCE].map(|_| random_input_resource(&mut rng));
        let mut output_resources = input_resources
            .iter()
            .map(|input| random_output_resource(&mut rng, input.get_nf().unwrap()))
            .collect::<Vec<_>>();
        let encoded_init_state = SudokuState::default().encode();
        let previous_state = SudokuState::default();
        let current_state = SudokuState::default();
        output_resources[0].kind.app_data_static =
            poseidon_hash(encoded_init_state, current_state.encode());
        output_resources[0].quantity = 1u64;
        let owned_resource_id = output_resources[0].commitment().inner();
        SudokuAppResourceLogicCircuit {
            owned_resource_id,
            input_resources,
            output_resources: output_resources.try_into().unwrap(),
            encoded_init_state,
            previous_state,
            current_state,
        }
    };
    let public_inputs = circuit.get_public_inputs(&mut rng);

    let prover =
        MockProver::<pallas::Base>::run(13, &circuit, vec![public_inputs.to_vec()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}

#[test]
fn test_halo2_sudoku_app_resource_logic_circuit_update() {
    use crate::app_resource_logic::tests::{random_input_resource, random_output_resource};
    use halo2_proofs::dev::MockProver;
    use rand::rngs::OsRng;

    let mut rng = OsRng;
    // Construct circuit
    let circuit = {
        let mut input_resources = [(); NUM_RESOURCE].map(|_| random_input_resource(&mut rng));
        let mut output_resources = input_resources
            .iter()
            .map(|input| random_output_resource(&mut rng, input.get_nf().unwrap()))
            .collect::<Vec<_>>();
        let init_state = SudokuState {
            state: [
                [5, 0, 1, 6, 7, 2, 4, 3, 9],
                [7, 0, 2, 8, 4, 3, 6, 5, 1],
                [3, 0, 4, 5, 9, 1, 7, 8, 2],
                [4, 0, 8, 9, 5, 7, 2, 1, 6],
                [2, 0, 6, 1, 8, 4, 9, 7, 3],
                [1, 0, 9, 3, 2, 6, 8, 4, 5],
                [8, 0, 5, 2, 1, 9, 3, 6, 7],
                [9, 0, 3, 7, 6, 8, 5, 2, 4],
                [6, 0, 7, 4, 3, 5, 1, 9, 8],
            ],
        };
        let encoded_init_state = init_state.encode();
        let previous_state = SudokuState {
            state: [
                [5, 8, 1, 6, 7, 2, 4, 3, 9],
                [7, 9, 2, 8, 4, 3, 6, 5, 1],
                [3, 0, 4, 5, 9, 1, 7, 8, 2],
                [4, 0, 8, 9, 5, 7, 2, 1, 6],
                [2, 0, 6, 1, 8, 4, 9, 7, 3],
                [1, 0, 9, 3, 2, 6, 8, 4, 5],
                [8, 0, 5, 2, 1, 9, 3, 6, 7],
                [9, 0, 3, 7, 6, 8, 5, 2, 4],
                [6, 0, 7, 4, 3, 5, 1, 9, 8],
            ],
        };
        let current_state = SudokuState {
            state: [
                [5, 8, 1, 6, 7, 2, 4, 3, 9],
                [7, 9, 2, 8, 4, 3, 6, 5, 1],
                [3, 6, 4, 5, 9, 1, 7, 8, 2],
                [4, 3, 8, 9, 5, 7, 2, 1, 6],
                [2, 0, 6, 1, 8, 4, 9, 7, 3],
                [1, 0, 9, 3, 2, 6, 8, 4, 5],
                [8, 0, 5, 2, 1, 9, 3, 6, 7],
                [9, 0, 3, 7, 6, 8, 5, 2, 4],
                [6, 0, 7, 4, 3, 5, 1, 9, 8],
            ],
        };
        input_resources[0].kind.app_data_static =
            poseidon_hash(encoded_init_state, previous_state.encode());
        input_resources[0].quantity = 1u64;
        output_resources[0].kind.app_data_static =
            poseidon_hash(encoded_init_state, current_state.encode());
        output_resources[0].quantity = 1u64;
        output_resources[0].kind.app_vk = input_resources[0].kind.app_vk;
        SudokuAppResourceLogicCircuit {
            owned_resource_id: input_resources[0].get_nf().unwrap().inner(),
            input_resources,
            output_resources: output_resources.try_into().unwrap(),
            encoded_init_state,
            previous_state,
            current_state,
        }
    };
    let public_inputs = circuit.get_public_inputs(&mut rng);

    let prover =
        MockProver::<pallas::Base>::run(13, &circuit, vec![public_inputs.to_vec()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}

#[test]
fn halo2_sudoku_app_resource_logic_circuit_final() {
    use crate::app_resource_logic::tests::{random_input_resource, random_output_resource};
    use halo2_proofs::dev::MockProver;

    let mut rng = OsRng;
    // Construct circuit
    let circuit = {
        let mut input_resources = [(); NUM_RESOURCE].map(|_| random_input_resource(&mut rng));
        let mut output_resources = input_resources
            .iter()
            .map(|input| random_output_resource(&mut rng, input.get_nf().unwrap()))
            .collect::<Vec<_>>();
        let init_state = SudokuState {
            state: [
                [5, 0, 1, 6, 7, 2, 4, 3, 9],
                [7, 0, 2, 8, 4, 3, 6, 5, 1],
                [3, 0, 4, 5, 9, 1, 7, 8, 2],
                [4, 0, 8, 9, 5, 7, 2, 1, 6],
                [2, 0, 6, 1, 8, 4, 9, 7, 3],
                [1, 0, 9, 3, 2, 6, 8, 4, 5],
                [8, 0, 5, 2, 1, 9, 3, 6, 7],
                [9, 0, 3, 7, 6, 8, 5, 2, 4],
                [6, 0, 7, 4, 3, 5, 1, 9, 8],
            ],
        };
        let encoded_init_state = init_state.encode();
        let previous_state = SudokuState {
            state: [
                [5, 8, 1, 6, 7, 2, 4, 3, 9],
                [7, 9, 2, 8, 4, 3, 6, 5, 1],
                [3, 0, 4, 5, 9, 1, 7, 8, 2],
                [4, 0, 8, 9, 5, 7, 2, 1, 6],
                [2, 0, 6, 1, 8, 4, 9, 7, 3],
                [1, 0, 9, 3, 2, 6, 8, 4, 5],
                [8, 0, 5, 2, 1, 9, 3, 6, 7],
                [9, 0, 3, 7, 6, 8, 5, 2, 4],
                [6, 0, 7, 4, 3, 5, 1, 9, 8],
            ],
        };
        let current_state = SudokuState {
            state: [
                [5, 8, 1, 6, 7, 2, 4, 3, 9],
                [7, 9, 2, 8, 4, 3, 6, 5, 1],
                [3, 6, 4, 5, 9, 1, 7, 8, 2],
                [4, 3, 8, 9, 5, 7, 2, 1, 6],
                [2, 5, 6, 1, 8, 4, 9, 7, 3],
                [1, 7, 9, 3, 2, 6, 8, 4, 5],
                [8, 4, 5, 2, 1, 9, 3, 6, 7],
                [9, 1, 3, 7, 6, 8, 5, 2, 4],
                [6, 2, 7, 4, 3, 5, 1, 9, 8],
            ],
        };
        input_resources[0].kind.app_data_static =
            poseidon_hash(encoded_init_state, previous_state.encode());
        input_resources[0].quantity = 1u64;
        output_resources[0].kind.app_data_static =
            poseidon_hash(encoded_init_state, current_state.encode());
        output_resources[0].quantity = 0u64;
        output_resources[0].kind.app_vk = input_resources[0].kind.app_vk;
        SudokuAppResourceLogicCircuit {
            owned_resource_id: input_resources[0].get_nf().unwrap().inner(),
            input_resources,
            output_resources: output_resources.try_into().unwrap(),
            encoded_init_state,
            previous_state,
            current_state,
        }
    };
    let public_inputs = circuit.get_public_inputs(&mut rng);

    let prover =
        MockProver::<pallas::Base>::run(13, &circuit, vec![public_inputs.to_vec()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
