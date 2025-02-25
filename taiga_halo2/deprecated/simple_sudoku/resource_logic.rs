use halo2_proofs::{
    circuit::{floor_planner, Layouter},
    plonk::{self, keygen_pk, keygen_vk, Circuit, ConstraintSystem, Error},
};
use pasta_curves::pallas;

extern crate taiga_halo2;
use taiga_halo2::{
    circuit::{
        resource_circuit::ResourceConfig,
        resource_logic_circuit::{
            BasicResourceLogicVariables, ResourceLogicVerifyingInfoTrait, ResourceLogicCircuit,
            ResourceLogicConfig, ResourceLogicInfo, ResourceLogicPublicInputs,
            ResourceLogicVerifyingInfo,
        },
    },
    constant::{NUM_RESOURCE, SETUP_PARAMS_MAP},
    resource::{Resource, RandomSeed},
    proof::Proof,
    resource_logic_circuit_impl,
    resource_logic_vk::ResourceLogicVerifyingKey,
};

use crate::circuit::{SudokuCircuit, SudokuConfig};
use rand::{rngs::OsRng, RngCore};

#[derive(Clone, Debug)]
pub struct SudokuResourceLogicConfig {
    resource_config: ResourceConfig,
    sudoku_config: SudokuConfig,
}

#[derive(Clone, Debug, Default)]
pub struct SudokuResourceLogic {
    pub sudoku: SudokuCircuit,
    input_resources: [Resource; NUM_RESOURCE],
    output_resources: [Resource; NUM_RESOURCE],
}

impl ResourceLogicCircuit for SudokuResourceLogic {
    fn custom_constraints(
        &self,
        config: ResourceLogicConfig,
        layouter: impl Layouter<pallas::Base>,
        _basic_variables: BasicResourceLogicVariables,
    ) -> Result<(), plonk::Error> {
        self.sudoku.synthesize(config.sudoku_config, layouter)
    }
}

impl ResourceLogicInfo for SudokuResourceLogic {
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
        pallas::Base::zero()
    }
}

impl SudokuResourceLogic {
    pub fn new(
        sudoku: SudokuCircuit,
        input_resources: [Resource; NUM_RESOURCE],
        output_resources: [Resource; NUM_RESOURCE],
    ) -> Self {
        Self {
            sudoku,
            input_resources,
            output_resources,
        }
    }
}

resource_logic_circuit_impl!(SudokuResourceLogic);

#[cfg(test)]
mod tests {
    use taiga_halo2::{
        constant::NUM_RESOURCE,
        resource::{Resource, RandomSeed},
        nullifier::{Nullifier, NullifierKeyContainer},
        resource_logic_vk::ResourceLogicVerifyingKey,
    };

    use ff::Field;
    use pasta_curves::pallas;
    use rand::rngs::OsRng;

    use halo2_proofs::{plonk, poly::commitment::Params};

    use crate::{circuit::SudokuCircuit, resource_logic::SudokuResourceLogic};

    #[test]
    fn test_resource_logic() {
        let mut rng = OsRng;
        let input_resources = [(); NUM_RESOURCE].map(|_| Resource::dummy(&mut rng));
        let output_resources = [(); NUM_RESOURCE].map(|_| Resource::dummy(&mut rng));

        const K: u32 = 13;
        let sudoku = SudokuCircuit {
            sudoku: [
                [7, 6, 9, 5, 3, 8, 1, 2, 4],
                [2, 4, 3, 7, 1, 9, 6, 5, 8],
                [8, 5, 1, 4, 6, 2, 9, 7, 3],
                [4, 8, 6, 9, 7, 5, 3, 1, 2],
                [5, 3, 7, 6, 2, 1, 4, 8, 9],
                [1, 9, 2, 8, 4, 3, 7, 6, 5],
                [6, 1, 8, 3, 5, 4, 2, 9, 7],
                [9, 7, 4, 2, 8, 6, 5, 3, 1],
                [3, 2, 5, 1, 9, 7, 8, 4, 6],
            ],
        };
        let params = Params::new(K);

        let vk = plonk::keygen_vk(&params, &sudoku).unwrap();

        let mut _resource_logic = SudokuResourceLogic::new(sudoku, input_resources, output_resources);

        let resource_logic_vk = ResourceLogicVerifyingKey::from_vk(vk);

        let app_data_static = pallas::Base::zero();
        let app_data_dynamic = pallas::Base::zero();

        let quantity: u64 = 0;
        let nk = NullifierKeyContainer::random_key(&mut rng);
        let rseed = RandomSeed::random(&mut rng);
        let rho = Nullifier::from(pallas::Base::random(&mut rng));
        Resource::new(
            resource_logic_vk,
            app_data_static,
            app_data_dynamic,
            quantity,
            nk,
            rho,
            true,
            rseed,
        );
    }
}
