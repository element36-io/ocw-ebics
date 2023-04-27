import { BN } from '@polkadot/util'
import React, { useEffect, useState } from 'react'
import { Dropdown, Form, Grid, Icon, Input, Label } from 'semantic-ui-react'
import { useSubstrateState } from './substrate-lib'
import { TxButton } from './substrate-lib/components'

// Destination options for transfer
const destinationOptions = [
  "IBAN",
  "Address",
  "Burn"
]

// Derive destination type from destination options
const deriveDest = (dest, addressTo, ibanTo) => {
  switch (dest) {
    case "IBAN":
      return {
          "Iban": ibanTo
      }
    case "Address":
      return {
          "Address": addressTo
      }
    default:
      return {
          "Burn": null
        }
  }
}

export default function Main(props) {
  const base = new BN(10).pow(new BN(10))
  
  const [status, setStatus] = useState(null)
  const [formState, setFormState] = useState({ addressTo: '', ibanTo: '', amount: 0, destination: destinationOptions[0] })

  const onChange = (_, data) =>
    setFormState(prev => ({ ...prev, [data.state]: data.value }))

  const { addressTo, amount, ibanTo, destination } = formState

  const { keyring, currentAccount, api, recipient } = useSubstrateState()
  const accounts = keyring.getPairs()

  const availableAccounts = []
  accounts.map(account => {
    return availableAccounts.push({
      key: account.meta.name,
      text: account.meta.name,
      value: account.address,
    })
  })

  useEffect(() => {
    let unsubscribe
    if (accounts) {
      let addressTo = recipient.address
      setFormState(prev => ({ ...prev, addressTo }))
      api.query.fiatRamps
        .accounts(addressTo, (result) => {
          if (result.isSome) {
            setFormState(prev => ({ ...prev, ibanTo: Buffer.from(result.unwrap()['iban'], "hex").toString() }))
          }
        })
        .then(unsub => {
          unsubscribe = unsub
        })
        .catch(console.error)
      
      return () => unsubscribe && unsubscribe()
    }
  }, [currentAccount?.address, api.query.fiatRamps])

  return (
    <Grid.Column width={8} textAlign="center">
      <h2>Donate via on-chain transaction</h2>
      <Form>
        <Form.Field>
          <Label basic color="teal">
            <Icon name="hand point right" />1 Unit = 1000000000000&nbsp;
          </Label>
          <Label
            basic
            color="teal"
            style={{ marginLeft: 0, marginTop: '.5em' }}
          >
            <Icon name="hand point right" />
            Transfer more than the existential amount for account with 0 balance
          </Label>
        </Form.Field>

        <Form.Field>
          <Dropdown
            placeholder="Transfer destination type"
            fluid
            selection
            labeled
            search
            options={destinationOptions.map((option) => {
              return {
                key: option,
                text: option,
                value: option,
              }
            })}
            text={`Transfer destination type: ${destination}`}
            state="destination"
            onChange={onChange}
          />
        </Form.Field>

        <Form.Field>
          {destination === "Address" &&
            <Input
              fluid
              label="To Address"
              type="text"
              placeholder="address"
              value={addressTo}
              state="addressTo"
              disabled
            />
          }
          {destination === "IBAN" &&
            <Input
              fluid
              label="To IBAN"
              type="text"
              placeholder="iban"
              value={ibanTo}
              state="ibanTo"
              onChange={onChange}
              disabled={ibanTo !== ""}
            />
          }
        </Form.Field>
        <Form.Field>
          <Input
            fluid
            label="Amount"
            type="number"
            state="amount"
            onChange={onChange}
          />
        </Form.Field>
        <Form.Field style={{ textAlign: 'center' }}>
          <TxButton
            label="Submit"
            type="SIGNED-TX"
            setStatus={setStatus}
            attrs={{
              palletRpc: 'fiatRamps',
              callable: 'transfer',
              inputParams: [base.mul(new BN(amount)), deriveDest(destination, addressTo, ibanTo)],
              paramFields: [true, true],
            }}
          />
        </Form.Field>
        <div style={{ overflowWrap: 'break-word' }}>{status}</div>
      </Form>
    </Grid.Column>
  )
}
