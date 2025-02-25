#[cfg(feature = "borsh")]
use crate::circuit::resource_logic_examples::TrivialResourceLogicCircuit;
#[cfg(feature = "examples")]
use crate::circuit::resource_logic_examples::{
    cascade_intent::CascadeIntentResourceLogicCircuit,
    or_relation_intent::OrRelationIntentResourceLogicCircuit,
    partial_fulfillment_intent::PartialFulfillmentIntentResourceLogicCircuit,
    receiver_resource_logic::ReceiverResourceLogicCircuit,
    signature_verification::SignatureVerificationResourceLogicCircuit,
    token::TokenResourceLogicCircuit,
};
use crate::error::TransactionError;
use crate::shielded_ptx::ResourceLogicVerifyingInfoSet;
use crate::{
    circuit::resource_logic_circuit::{
        ResourceLogicVerifyingInfo, ResourceLogicVerifyingInfoTrait, VampIRResourceLogicCircuit,
    },
    constant::{
        RESOURCE_LOGIC_CIRCUIT_NULLIFIER_ONE_PUBLIC_INPUT_IDX,
        RESOURCE_LOGIC_CIRCUIT_NULLIFIER_TWO_PUBLIC_INPUT_IDX,
        RESOURCE_LOGIC_CIRCUIT_OUTPUT_CM_ONE_PUBLIC_INPUT_IDX,
        RESOURCE_LOGIC_CIRCUIT_OUTPUT_CM_TWO_PUBLIC_INPUT_IDX,
        RESOURCE_LOGIC_CIRCUIT_OWNED_RESOURCE_ID_PUBLIC_INPUT_IDX,
    },
    nullifier::Nullifier,
    resource::ResourceCommitment,
};

#[cfg(feature = "borsh")]
use borsh::{BorshDeserialize, BorshSerialize};
use pasta_curves::pallas;
#[cfg(feature = "serde")]
use serde;
use std::path::PathBuf;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "borsh", derive(BorshSerialize, BorshDeserialize))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ResourceLogicRepresentation {
    // vampir has a unified circuit representation.
    VampIR(Vec<u8>),
    // Native halo2 circuits don't have a unified representatioin, enumerate the resource_logic circuit examples for the moment.
    // TODO: figure out if we can have a unified circuit presentation. In theory, it's possible to separate the circuit system and proving system.
    Trivial,
    Token,
    SignatureVerification,
    Receiver,
    PartialFulfillmentIntent,
    OrRelationIntent,
    CascadeIntent,
    // Add other native resource_logic types here if needed
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "borsh", derive(BorshSerialize, BorshDeserialize))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResourceLogicByteCode {
    circuit: ResourceLogicRepresentation,
    inputs: Vec<u8>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "borsh", derive(BorshSerialize, BorshDeserialize))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ApplicationByteCode {
    app_resource_logic_bytecode: ResourceLogicByteCode,
    dynamic_resource_logic_bytecode: Vec<ResourceLogicByteCode>,
}

impl ResourceLogicByteCode {
    pub fn new(circuit: ResourceLogicRepresentation, inputs: Vec<u8>) -> Self {
        Self { circuit, inputs }
    }

    pub fn generate_proof(self) -> Result<ResourceLogicVerifyingInfo, TransactionError> {
        match self.circuit {
            ResourceLogicRepresentation::VampIR(circuit) => {
                // TDDO: use the file_name api atm,
                // request vamp_ir to provide a api to generate circuit from bytes.
                let vamp_ir_circuit_file =
                    PathBuf::from(String::from_utf8_lossy(&circuit).to_string());
                let inputs_file = PathBuf::from(String::from_utf8_lossy(&self.inputs).to_string());
                let resource_logic_circuit = VampIRResourceLogicCircuit::from_vamp_ir_file(
                    &vamp_ir_circuit_file,
                    &inputs_file,
                );
                Ok(resource_logic_circuit.get_verifying_info())
            }
            #[cfg(feature = "borsh")]
            ResourceLogicRepresentation::Trivial => {
                let resource_logic = TrivialResourceLogicCircuit::from_bytes(&self.inputs);
                Ok(resource_logic.get_verifying_info())
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::Token => {
                let resource_logic = TokenResourceLogicCircuit::from_bytes(&self.inputs);
                Ok(resource_logic.get_verifying_info())
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::SignatureVerification => {
                let resource_logic =
                    SignatureVerificationResourceLogicCircuit::from_bytes(&self.inputs);
                Ok(resource_logic.get_verifying_info())
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::Receiver => {
                let resource_logic = ReceiverResourceLogicCircuit::from_bytes(&self.inputs);
                Ok(resource_logic.get_verifying_info())
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::PartialFulfillmentIntent => {
                let resource_logic =
                    PartialFulfillmentIntentResourceLogicCircuit::from_bytes(&self.inputs);
                Ok(resource_logic.get_verifying_info())
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::OrRelationIntent => {
                let resource_logic = OrRelationIntentResourceLogicCircuit::from_bytes(&self.inputs);
                Ok(resource_logic.get_verifying_info())
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::CascadeIntent => {
                let resource_logic = CascadeIntentResourceLogicCircuit::from_bytes(&self.inputs);
                Ok(resource_logic.get_verifying_info())
            }
            #[allow(unreachable_patterns)]
            _ => Err(TransactionError::InvalidResourceLogicRepresentation),
        }
    }

    // Verify resource_logic circuit transparently and return owned resource PubID for further checking
    pub fn verify_transparently(
        &self,
        compliance_nfs: &[Nullifier],
        compliance_cms: &[ResourceCommitment],
    ) -> Result<pallas::Base, TransactionError> {
        // check resource logic transparently
        let public_inputs = match &self.circuit {
            ResourceLogicRepresentation::VampIR(circuit) => {
                // TDDO: use the file_name api atm,
                // request vamp_ir to provide a api to generate circuit from bytes.
                let vamp_ir_circuit_file =
                    PathBuf::from(String::from_utf8_lossy(circuit).to_string());
                let inputs_file = PathBuf::from(String::from_utf8_lossy(&self.inputs).to_string());
                let resource_logic_circuit = VampIRResourceLogicCircuit::from_vamp_ir_file(
                    &vamp_ir_circuit_file,
                    &inputs_file,
                );
                resource_logic_circuit.verify_transparently()?
            }
            #[cfg(feature = "borsh")]
            ResourceLogicRepresentation::Trivial => {
                let resource_logic = TrivialResourceLogicCircuit::from_bytes(&self.inputs);
                resource_logic.verify_transparently()?
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::Token => {
                let resource_logic = TokenResourceLogicCircuit::from_bytes(&self.inputs);
                resource_logic.verify_transparently()?
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::SignatureVerification => {
                let resource_logic =
                    SignatureVerificationResourceLogicCircuit::from_bytes(&self.inputs);
                resource_logic.verify_transparently()?
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::Receiver => {
                let resource_logic = ReceiverResourceLogicCircuit::from_bytes(&self.inputs);
                resource_logic.verify_transparently()?
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::PartialFulfillmentIntent => {
                let resource_logic =
                    PartialFulfillmentIntentResourceLogicCircuit::from_bytes(&self.inputs);
                resource_logic.verify_transparently()?
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::OrRelationIntent => {
                let resource_logic = OrRelationIntentResourceLogicCircuit::from_bytes(&self.inputs);
                resource_logic.verify_transparently()?
            }
            #[cfg(feature = "examples")]
            ResourceLogicRepresentation::CascadeIntent => {
                let resource_logic = CascadeIntentResourceLogicCircuit::from_bytes(&self.inputs);
                resource_logic.verify_transparently()?
            }
            #[allow(unreachable_patterns)]
            _ => return Err(TransactionError::InvalidResourceLogicRepresentation),
        };

        // check nullifiers
        // Check the resource_logic actually uses the input resources from compliance circuits.
        let resource_logic_nfs = [
            public_inputs.get_from_index(RESOURCE_LOGIC_CIRCUIT_NULLIFIER_ONE_PUBLIC_INPUT_IDX),
            public_inputs.get_from_index(RESOURCE_LOGIC_CIRCUIT_NULLIFIER_TWO_PUBLIC_INPUT_IDX),
        ];

        if !((compliance_nfs[0].inner() == resource_logic_nfs[0]
            && compliance_nfs[1].inner() == resource_logic_nfs[1])
            || (compliance_nfs[0].inner() == resource_logic_nfs[1]
                && compliance_nfs[1].inner() == resource_logic_nfs[0]))
        {
            return Err(TransactionError::InconsistentNullifier);
        }

        // check resource_commitments
        // Check the resource_logic actually uses the output resources from compliance circuits.
        let resource_logic_cms = [
            public_inputs.get_from_index(RESOURCE_LOGIC_CIRCUIT_OUTPUT_CM_ONE_PUBLIC_INPUT_IDX),
            public_inputs.get_from_index(RESOURCE_LOGIC_CIRCUIT_OUTPUT_CM_TWO_PUBLIC_INPUT_IDX),
        ];
        if !((compliance_cms[0].inner() == resource_logic_cms[0]
            && compliance_cms[1].inner() == resource_logic_cms[1])
            || (compliance_cms[0].inner() == resource_logic_cms[1]
                && compliance_cms[1].inner() == resource_logic_cms[0]))
        {
            return Err(TransactionError::InconsistentOutputResourceCommitment);
        }

        Ok(public_inputs.get_from_index(RESOURCE_LOGIC_CIRCUIT_OWNED_RESOURCE_ID_PUBLIC_INPUT_IDX))
    }
}

impl ApplicationByteCode {
    pub fn new(
        app_resource_logic_bytecode: ResourceLogicByteCode,
        dynamic_resource_logic_bytecode: Vec<ResourceLogicByteCode>,
    ) -> Self {
        Self {
            app_resource_logic_bytecode,
            dynamic_resource_logic_bytecode,
        }
    }

    pub fn generate_proofs(self) -> Result<ResourceLogicVerifyingInfoSet, TransactionError> {
        let app_resource_logic_verifying_info =
            self.app_resource_logic_bytecode.generate_proof()?;

        let app_dynamic_resource_logic_verifying_info: Result<Vec<_>, _> = self
            .dynamic_resource_logic_bytecode
            .into_iter()
            .map(|bytecode| bytecode.generate_proof())
            .collect();
        Ok(ResourceLogicVerifyingInfoSet::new(
            app_resource_logic_verifying_info,
            app_dynamic_resource_logic_verifying_info?,
        ))
    }

    // Verify resource_logic circuits transparently and return owned resource PubID for further checking
    pub fn verify_transparently(
        &self,
        compliance_nfs: &[Nullifier],
        compliance_cms: &[ResourceCommitment],
    ) -> Result<pallas::Base, TransactionError> {
        let owned_resource_id = self
            .app_resource_logic_bytecode
            .verify_transparently(compliance_nfs, compliance_cms)?;
        for dynamic_resource_logic in self.dynamic_resource_logic_bytecode.iter() {
            let id = dynamic_resource_logic.verify_transparently(compliance_nfs, compliance_cms)?;
            // check: the app_resource_logic and dynamic_resource_logics belong to the resource
            if id != owned_resource_id {
                return Err(TransactionError::InconsistentOwnedResourceID);
            }
        }
        Ok(owned_resource_id)
    }
}
