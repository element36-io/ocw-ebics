import React, { useEffect, useState } from 'react'
import { Card, Form, FormRadio, Grid, Input, Radio } from 'semantic-ui-react'

import { useSubstrateState } from './substrate-lib'
import { TxButton } from './substrate-lib/components'

function Main(props) {
  const { api, currentAccount } = useSubstrateState()

  // The transaction submission status
  const [status, setStatus] = useState('')

  // The currently stored value
  const [currentValue, setCurrentValue] = useState(0)
  const [formIban, setFormIban] = useState('')
  const [formAccountBehavior, setFormAccountBehavior] = useState(0)

  console.log("" + currentAccount?.address)

  useEffect(() => {
    let unsubscribe
    api.query.fiatRamps
      .accounts(currentAccount?.address, newValue => {
        // The storage value is an Option<u32>
        // So we have to check whether it is None first
        // There is also unwrapOr
        if (newValue.isNone) {
          setCurrentValue('<None>')
        } else {
          setCurrentValue(newValue.unwrap().toNumber())
        }
      })
      .then(unsub => {
        unsubscribe = unsub
      })
      .catch(console.error)

    return () => unsubscribe && unsubscribe()
  }, [api.query.fiatRamps, currentAccount?.address])

  return (
    <Grid.Column textAlign="center" width={16}>
      <h1>Buy Me a Coffee</h1>
      {currentValue === '<None>' &&
        <>
          <Card centered fluid>
            <Card.Content textAlign="center">
              No account associated with this address:
                <br/>
                <br/>
              <b>{currentAccount.address}</b>
            </Card.Content>
          </Card>
          <Form>
            <Form.Field width={8}>
              <Input
                label="IBAN"
                state="iban"
                type="string"
                onChange={(_, { value }) => setFormIban(value)}
              />
              <p>
                <b>Account Behavior</b>
              </p>
              <FormRadio
                label="Keep"
                state="accountBehaviorKeep"
                type="radio"
                onChange={(_, { value }) => setFormAccountBehavior(value ? 0 : 1)}
              />
              <Radio
                label="Ping"
                state="accountBehaviorPing"
                type="radio"
                onChange={(_, { value }) => setFormAccountBehavior(value ? 1 : 0)}
              />
            </Form.Field>
            <Form.Field style={{ textAlign: 'center' }}>
              <TxButton
                label="Create Account"
                type="SIGNED-TX"
                setStatus={setStatus}
                attrs={{
                  palletRpc: 'fiatRamps',
                  callable: 'createAccount',
                  inputParams: [formIban, formAccountBehavior],
                  paramFields: [true, true],
                }}
              />
            </Form.Field>

          <div style={{ overflowWrap: 'break-word' }}>{status}</div>
        </Form>
        </>
      }
    </Grid.Column>
  )
}

export default function TemplateModule(props) {
  const { api } = useSubstrateState()
  return api.query.fiatRamps && api.query.fiatRamps.accounts ? (
    <Main {...props} />
  ) : null
}
