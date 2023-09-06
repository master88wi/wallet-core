use crate::{aliases::*, BitcoinEntry};
use tw_coin_entry::coin_entry::CoinEntry;
use tw_coin_entry::modules::plan_builder::PlanBuilder;
use tw_proto::BitcoinV2::Proto;
use tw_proto::Utxo::Proto as UtxoProto;

pub struct BitcoinPlanBuilder;

impl PlanBuilder for BitcoinPlanBuilder {
    type SigningInput<'a> = Proto::ComposePlan<'a>;
    type Plan = Proto::TransactionPlan<'static>;

    #[inline]
    fn plan(
        &self,
        _coin: &dyn tw_coin_entry::coin_context::CoinContext,
        _input: Self::SigningInput<'_>,
    ) -> Self::Plan {
        todo!()
    }
}

impl BitcoinPlanBuilder {
    fn plan_brc20(
        &self,
        _coin: &dyn tw_coin_entry::coin_context::CoinContext,
        proto: Proto::mod_ComposePlan::ComposeBrc20Plan<'static>,
    ) -> Proto::mod_TransactionPlan::Brc20Plan<'static> {
        let brc20_info = proto.inscription.unwrap();
        let tagged_output = super::utils::hard_clone_proto_output(proto.tagged_output.unwrap());

        // First, we create the reveal transaction in order to calculate its input requirement (fee + dust limit).

        // We can use a zeroed Txid here.
        let txid = vec![0; 32];
        let brc20_input = Proto::Input {
            txid: txid.into(),
            sighash_type: UtxoProto::SighashType::UseDefault,
            to_recipient: ProtoInputRecipient::builder(Proto::mod_Input::InputBuilder {
                variant: ProtoInputBuilder::brc20_inscribe(brc20_info.clone()),
            }),
            ..Default::default()
        };

        let reveal_signing = Proto::SigningInput {
            inputs: vec![brc20_input],
            outputs: vec![tagged_output.clone()],
            input_selector: UtxoProto::InputSelector::UseAll,
            disable_change_output: true,
            ..Default::default()
        };

        // We can now determine the fee of the reveal transaction.
        let presigned = BitcoinEntry.preimage_hashes(_coin, reveal_signing.clone());
        let fee_estimate = presigned.fee_estimate;

        // Create the BRC20 output for the COMMIT transaction; we set the
        // amount to the estimated fee (REVEAL) plus the dust limit (`tagged_output.value`).
        let brc20_output = Proto::Output {
            value: fee_estimate + tagged_output.value,
            to_recipient: ProtoOutputRecipient::builder(Proto::mod_Output::OutputBuilder {
                variant: ProtoOutputBuilder::brc20_inscribe(
                    Proto::mod_Output::OutputBrc20Inscription {
                        inscribe_to: brc20_info.inscribe_to.to_vec().into(),
                        ticker: brc20_info.ticker.to_string().into(),
                        transfer_amount: brc20_info.transfer_amount,
                    },
                ),
            }),
        };

        // Create the full COMMIT transaction with the appropriately selected inputs.
        let commit_signing = Proto::SigningInput {
            inputs: proto
                .inputs
                .into_iter()
                .map(super::utils::hard_clone_proto_input)
                .collect(),
            outputs: vec![brc20_output],
            input_selector: proto.input_selector,
            disable_change_output: proto.disable_change_output,
            ..Default::default()
        };

        // We now determine the Txid of the COMMIT transaction, which we will have
        // to use in the REVEAL transaction.
        let presigned = BitcoinEntry.preimage_hashes(_coin, commit_signing.clone());
        let commit_txid: Vec<u8> = presigned.txid.to_vec().iter().copied().rev().collect();

        // Now we construct the *actual* REVEAL transaction. Note that we use the
        let brc20_input = Proto::Input {
            txid: commit_txid.into(), // Reference COMMIT transaction.
            sighash_type: UtxoProto::SighashType::UseDefault,
            to_recipient: ProtoInputRecipient::builder(Proto::mod_Input::InputBuilder {
                variant: ProtoInputBuilder::brc20_inscribe(brc20_info.clone()),
            }),
            ..Default::default()
        };

        // Build the full (unsigned) REVEAL transaction.
        let reveal_signing = Proto::SigningInput {
            inputs: vec![brc20_input],
            outputs: vec![tagged_output],
            input_selector: UtxoProto::InputSelector::UseAll,
            disable_change_output: true,
            ..Default::default()
        };

        Proto::mod_TransactionPlan::Brc20Plan {
            commit: Some(commit_signing),
            reveal: Some(reveal_signing),
        }
    }
}