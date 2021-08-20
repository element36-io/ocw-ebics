#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};

use sp_std::vec::Vec;

mod fiat_ramps_type {
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
    pub struct Transaction {
        iban: Vec<u8>,
        name: Vec<u8>,
        addr_line: Vec<Vec<u8>>,
        currency: Vec<u8>,
        amount: f64,
        reference: Vec<u8>,
        pmt_inf_id: Vec<u8>,
        msg_id: Vec<u8>,
        instr_id: Vec<u8>
    }
    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
    pub struct Iban {
        iban: Vec<u8>,
        balance_op: f64,
        balance_op_currency: Vec<u8>,
        balance_cl: f64,
        balance_cl_currency: Vec<u8>,
        booking_date: Vec<u8>,
        validation_date: Vec<u8>,
        incoming_transactions: Vec<Transaction>,
        outgoing_transactions: Vec<Transaction>
    }

    impl Transaction {
        pub fn new(
            iban: <Vec<u8>>,
            name: Vec<u8>,
            addr_line: Vec<Vec<u8>>,
            currency: Vec<u8>,
            amount: f64,
            reference: Vec<u8>,
            pmt_inf_id: Vec<u8>,
            msg_id: Vec<u8>,
            instr_id: Vec<u8>
        ) {
            Self {
                iban,
                name,
                addr_line,
                currency,
                amount,
                reference,
                pmt_inf_id,
                msg_id,
                instr_id,
            }
        }

        pub fn parse_from_utf8()
    }

    impl Iban {
        pub fn new(
            iban: Vec<u8>,
            balance_op: f64,
            balance_op_currency: Vec<u8>,
            balance_cl: f64,
            balance_cl_currency: Vec<u8>,
            booking_date: Vec<u8>,
            validation_date: Vec<u8>,
            incoming_transactions: Vec<Transaction>,
            outgoing_transactions: Vec<Transaction>
        ) {
            Self {
                iban,
                balance_op,
                balance_op_currency,
                balance_cl,
                balance_cl_currency,
                booking_date,
                validation_date,
                incoming_transactions,
                outgoing_transactions,
            }
        }
    }
}