use crate::circuit::{
    gadgets::{
        conditional_equal::ConditionalEqualConfig,
        mul::{MulChip, MulInstructions},
        poseidon_hash::poseidon_hash_gadget,
        sub::{SubChip, SubInstructions},
    },
    resource_logic_circuit::BasicResourceLogicVariables,
};
use halo2_gadgets::poseidon::Pow5Config as PoseidonConfig;
use halo2_proofs::{
    circuit::{AssignedCell, Layouter},
    plonk::Error,
};
use pasta_curves::pallas;

#[derive(Clone, Debug)]
pub struct PartialFulfillmentIntentLabel {
    pub token_resource_logic_vk: AssignedCell<pallas::Base, pallas::Base>,
    pub sold_token: AssignedCell<pallas::Base, pallas::Base>,
    pub sold_token_quantity: AssignedCell<pallas::Base, pallas::Base>,
    pub bought_token: AssignedCell<pallas::Base, pallas::Base>,
    pub bought_token_quantity: AssignedCell<pallas::Base, pallas::Base>,
    pub receiver_npk: AssignedCell<pallas::Base, pallas::Base>,
    pub receiver_value: AssignedCell<pallas::Base, pallas::Base>,
}

impl PartialFulfillmentIntentLabel {
    pub fn encode(
        &self,
        config: PoseidonConfig<pallas::Base, 3, 2>,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<AssignedCell<pallas::Base, pallas::Base>, Error> {
        // Encode the label of intent resource
        poseidon_hash_gadget(
            config.clone(),
            layouter.namespace(|| "label encoding"),
            [
                self.sold_token.clone(),
                self.sold_token_quantity.clone(),
                self.bought_token.clone(),
                self.bought_token_quantity.clone(),
                self.token_resource_logic_vk.clone(),
                self.receiver_npk.clone(),
                self.receiver_value.clone(),
            ],
        )
    }

    /// Checks to be enforced if `is_input_resource == 1`
    pub fn is_input_resource_checks(
        &self,
        is_input_resource: &AssignedCell<pallas::Base, pallas::Base>,
        basic_variables: &BasicResourceLogicVariables,
        config: &ConditionalEqualConfig,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "conditional equal: check bought token vk",
            |mut region| {
                config.assign_region(
                    is_input_resource,
                    &self.token_resource_logic_vk,
                    &basic_variables.output_resource_variables[0]
                        .resource_variables
                        .logic,
                    0,
                    &mut region,
                )
            },
        )?;

        layouter.assign_region(
            || "conditional equal: check bought token vk",
            |mut region| {
                config.assign_region(
                    is_input_resource,
                    &self.bought_token,
                    &basic_variables.output_resource_variables[0]
                        .resource_variables
                        .label,
                    0,
                    &mut region,
                )
            },
        )?;

        // check npk
        layouter.assign_region(
            || "conditional equal: check bought token npk",
            |mut region| {
                config.assign_region(
                    is_input_resource,
                    &self.receiver_npk,
                    &basic_variables.output_resource_variables[0]
                        .resource_variables
                        .npk,
                    0,
                    &mut region,
                )
            },
        )?;

        // check value
        layouter.assign_region(
            || "conditional equal: check bought token value",
            |mut region| {
                config.assign_region(
                    is_input_resource,
                    &self.receiver_value,
                    &basic_variables.output_resource_variables[0]
                        .resource_variables
                        .value,
                    0,
                    &mut region,
                )
            },
        )?;

        Ok(())
    }

    /// Checks to be enforced if `is_output_resource == 1`
    pub fn is_output_resource_checks(
        &self,
        is_output_resource: &AssignedCell<pallas::Base, pallas::Base>,
        basic_variables: &BasicResourceLogicVariables,
        config: &ConditionalEqualConfig,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "conditional equal: check sold token resource_logic_vk",
            |mut region| {
                config.assign_region(
                    is_output_resource,
                    &self.token_resource_logic_vk,
                    &basic_variables.input_resource_variables[0]
                        .resource_variables
                        .logic,
                    0,
                    &mut region,
                )
            },
        )?;

        layouter.assign_region(
            || "conditional equal: check sold token label",
            |mut region| {
                config.assign_region(
                    is_output_resource,
                    &self.sold_token,
                    &basic_variables.input_resource_variables[0]
                        .resource_variables
                        .label,
                    0,
                    &mut region,
                )
            },
        )?;

        layouter.assign_region(
            || "conditional equal: check sold token quantity",
            |mut region| {
                config.assign_region(
                    is_output_resource,
                    &self.sold_token_quantity,
                    &basic_variables.input_resource_variables[0]
                        .resource_variables
                        .quantity,
                    0,
                    &mut region,
                )
            },
        )?;

        Ok(())
    }

    /// Checks to be enforced if `is_partial_fulfillment == 1`
    pub fn is_partial_fulfillment_checks(
        &self,
        is_input_resource: &AssignedCell<pallas::Base, pallas::Base>,
        basic_variables: &BasicResourceLogicVariables,
        config: &ConditionalEqualConfig,
        sub_chip: &SubChip<pallas::Base>,
        mul_chip: &MulChip<pallas::Base>,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        let is_partial_fulfillment = {
            let is_partial_fulfillment = SubInstructions::sub(
                sub_chip,
                layouter
                    .namespace(|| "expected_bought_token_quantity - actual_bought_token_quantity"),
                &self.bought_token_quantity,
                &basic_variables.output_resource_variables[0]
                    .resource_variables
                    .quantity,
            )?;
            MulInstructions::mul(
                mul_chip,
                layouter.namespace(|| "is_input * is_partial_fulfillment"),
                is_input_resource,
                &is_partial_fulfillment,
            )?
        };

        // check returned token vk if it's partially fulfilled
        layouter.assign_region(
            || "conditional equal: check returned token vk",
            |mut region| {
                config.assign_region(
                    &is_partial_fulfillment,
                    &self.token_resource_logic_vk,
                    &basic_variables.output_resource_variables[1]
                        .resource_variables
                        .logic,
                    0,
                    &mut region,
                )
            },
        )?;

        // check return token label if it's partially fulfilled
        layouter.assign_region(
            || "conditional equal: check returned token label",
            |mut region| {
                config.assign_region(
                    &is_partial_fulfillment,
                    &self.sold_token,
                    &basic_variables.output_resource_variables[1]
                        .resource_variables
                        .label,
                    0,
                    &mut region,
                )
            },
        )?;

        layouter.assign_region(
            || "conditional equal: check returned token npk",
            |mut region| {
                config.assign_region(
                    &is_partial_fulfillment,
                    &self.receiver_npk,
                    &basic_variables.output_resource_variables[1]
                        .resource_variables
                        .npk,
                    0,
                    &mut region,
                )
            },
        )?;

        layouter.assign_region(
            || "conditional equal: check returned token value",
            |mut region| {
                config.assign_region(
                    &is_partial_fulfillment,
                    &self.receiver_value,
                    &basic_variables.output_resource_variables[1]
                        .resource_variables
                        .value,
                    0,
                    &mut region,
                )
            },
        )?;

        // quantity check
        {
            let actual_sold_quantity = SubInstructions::sub(
                sub_chip,
                layouter.namespace(|| "expected_sold_quantity - returned_quantity"),
                &self.sold_token_quantity,
                &basic_variables.output_resource_variables[1]
                    .resource_variables
                    .quantity,
            )?;

            // check (expected_bought_quantity * actual_sold_quantity) == (expected_sold_quantity * actual_bought_quantity)
            // if it's partially fulfilled
            let expected_bought_mul_actual_sold_quantity = MulInstructions::mul(
                mul_chip,
                layouter.namespace(|| "expected_bought_quantity * actual_sold_quantity"),
                &self.bought_token_quantity,
                &actual_sold_quantity,
            )?;
            let expected_sold_mul_actual_bought_quantity = MulInstructions::mul(
                mul_chip,
                layouter.namespace(|| "expected_sold_quantity * actual_bought_quantity"),
                &self.sold_token_quantity,
                &basic_variables.output_resource_variables[0]
                    .resource_variables
                    .quantity,
            )?;

            layouter.assign_region(
                    || "conditional equal: expected_bought_quantity * actual_sold_quantity == expected_sold_quantity * actual_bought_quantity",
                    |mut region| {
                        config.assign_region(
                            &is_partial_fulfillment,
                            &expected_bought_mul_actual_sold_quantity,
                            &expected_sold_mul_actual_bought_quantity,
                            0,
                            &mut region,
                        )
                    },
                )?;
        }

        Ok(())
    }
}
