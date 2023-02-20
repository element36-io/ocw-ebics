import React, { useEffect, useState } from 'react'
import { Card, Form, Grid, Input } from 'semantic-ui-react'

import { useSubstrateState } from './substrate-lib'
import { TxButton } from './substrate-lib/components'

function Main(props) {
  const { api, currentAccount } = useSubstrateState()

  // The transaction submission status
  const [status, setStatus] = useState('')

  // The currently stored value
  const [currentValue, setCurrentValue] = useState({})
  const [formIban, setFormIban] = useState('')

  console.log("CurAccount" + currentAccount?.address)

  useEffect(() => {
    let unsubscribe
    api.query.fiatRamps
      .accounts(currentAccount?.address, newValue => {
        // The storage value is an Option, so we need to check if it has a value
        if (newValue.isNone) {
          setCurrentValue({})
        } else {
          setCurrentValue({
            iban: Buffer.from(newValue.unwrap()["iban"], "hex").toString(),
            behaviour: Object.keys(newValue.unwrap()["behaviour"].toHuman())
          })
        }
      })
      .then(unsub => {
        unsubscribe = unsub
      })
      .catch(console.error)

    return () => unsubscribe && unsubscribe()
  }, [api.query.fiatRamps, currentAccount?.address])

  console.log("Current Value: " + JSON.stringify(currentValue))

  return (
    <Grid.Column textAlign="center" width={16}>
      <h1>EBICS Bank</h1>
      {!Object.keys(currentValue).length ?
        <>
          <Card centered fluid>
            <Card.Content textAlign="center">
              No account associated with your address:
                <br/>
                <br/>
              <b>{currentAccount.address}</b>
            </Card.Content>
          </Card>
          <Form>
            <h3>Map your IBAN to your address</h3>
            <Form.Field>
              <Input
                label="IBAN"
                state="iban"
                type="string"
                onChange={(_, { value }) => setFormIban(value)}
              />
            </Form.Field>
            {/* <Form.Field>
              <h4>Select behaviour of your account</h4>
              <p>
                <b>Keep</b> - the received funds will be kept on the account
                <br/>
                <b>Ping</b> - the received funds will be sent to the specified IBAN 
              </p>
              <Radio
                label="Keep"
                state="accountBehaviorKeep"
                toggle={true}
                type="radio"
                onChange={(_, { value }) => setFormAccountBehavior(value ? 0 : 1)}
              />
              <br/>
              <Radio
                label="Ping"
                state="accountBehaviorPing"
                toggle={true}
                type="radio"
                onChange={(_, { value }) => setFormAccountBehavior(value ? 1 : 0)}
              />
            </Form.Field> */}
            <Form.Field style={{ textAlign: 'center' }}>
              <TxButton
                label="Register Bank Account"
                type="SIGNED-TX"
                setStatus={setStatus}
                attrs={{
                  palletRpc: 'fiatRamps',
                  callable: 'createAccount',
                  inputParams: [formIban, 0x00],
                  paramFields: [true, true],
                }}
              />
            </Form.Field>
          <div style={{ overflowWrap: 'break-word' }}>{status}</div>
        </Form>
        </>
        : (
          <>
            <Card centered fluid>
              <Card.Content textAlign="center">
                <p>
                  <b>IBAN</b>
                </p>
                <p>{currentValue.iban}</p>
                <p>
                  <b>Account Behavior</b>
                </p>
                <p>{currentValue.behaviour}</p>
                
              </Card.Content>
            </Card>
            </>
        )
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
