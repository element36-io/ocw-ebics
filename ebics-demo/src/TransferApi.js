import React, { useEffect, useState } from 'react'
import { Dropdown, Form, Grid, Input } from 'semantic-ui-react'
import { useSubstrateState } from './substrate-lib'
import { TxButton } from './substrate-lib/components'

// Destination options for transfer
const destinationOptions = [
  "IBAN",
  "Address",
]

export default function Main(props) {
  const [status, setStatus] = useState(null)
  const [formState, setFormState] = useState({ addressTo: '', ibanTo: '', amount: 0, destination: destinationOptions[0] })
  const [availableIbans, setAvailableIbans] = useState([])

  const onChange = (_, data) =>
    setFormState(prev => ({ ...prev, [data.state]: data.value }))

  const { addressTo, amount, ibanTo, destination } = formState

  const { keyring, api } = useSubstrateState()
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
    if (api.query.fiatRamps) {
        api.query.fiatRamps.accounts.entries()
            .then((accounts) => {
                setAvailableIbans(accounts.map(account => {
                    return {
                        key: Buffer.from(account[1].unwrap()['iban'], "hex").toString(),
                        text: Buffer.from(account[1].unwrap()['iban'], "hex").toString(),
                        value: Buffer.from(account[1].unwrap()['iban'], "hex").toString(),
                    }
                }))
            })
    }
  }, [api.query.fiatRamps, api.query.fiatRamps.accounts])

  return (
    <Grid.Column width={8} textAlign="center">
      <h1>Transfer via EBICS API</h1>
      <Form>
        <Form.Field>
            <Dropdown
                placeholder="Select from available addresses"
                fluid
                selection
                search
                options={availableIbans}
                state="addressTo"
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
              onChange={onChange}
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
              palletRpc: 'balances',
              callable: 'transfer',
              inputParams: [addressTo, amount],
              paramFields: [true, true],
            }}
          />
        </Form.Field>
        <div style={{ overflowWrap: 'break-word' }}>{status}</div>
      </Form>
    </Grid.Column>
  )
}
