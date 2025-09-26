import React, { useEffect, useState, useRef } from 'react'
import { Button, Message, Grid, Card, List } from 'semantic-ui-react'

export default function StatementsAndProofs() {
  const [statements, setStatements] = useState([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState(null)
  const hasFetched = useRef(false) // Track if fetch has occurred

  useEffect(() => {
    const fetchStatements = async () => {
      if (hasFetched.current) return // Skip if already fetched
      hasFetched.current = true

      setLoading(true)
      setError(null)
      try {
        const res = await fetch(
          'http://w.e36.io:8093/ebics/api-v1/bankstatements'
        )
        if (!res.ok) throw new Error(`Failed: ${res.status}`)
        const data = await res.json()
        setStatements(data)
      } catch (err) {
        setError('Failed to fetch statements: ' + err.message)
      } finally {
        setLoading(false)
      }
    }

    fetchStatements()
  }, [])

  const openProof = receiptUrl => {
    try {
      // Ensure receiptUrl is absolute; prepend base URL if relative
      const absoluteUrl = receiptUrl.startsWith('http')
        ? receiptUrl
        : `http://w.e36.io:8093/ebics/${receiptUrl}`
      window.open(absoluteUrl, '_blank', 'noopener,noreferrer')
    } catch (err) {
      setError('Failed to open proof: ' + err.message)
    }
  }

  return (
    <Grid.Column>
      <Card fluid>
        <Card.Content>
          <Card.Header>Bank Statements</Card.Header>
          <Card.Description>
            View fetched statements and download ZK Proof receipts.
          </Card.Description>

          {loading && <p>Loading statements…</p>}

          {!loading && statements.length > 0 && (
            <List divided relaxed>
              {statements.map((s, idx) => (
                <List.Item key={idx}>
                  <List.Icon
                    name="file alternate outline"
                    size="large"
                    verticalAlign="middle"
                  />
                  <List.Content>
                    <List.Header as="a">
                      Account: {s.account?.iban || 'Unknown'}
                    </List.Header>
                    <List.Description>
                      {s.transactions?.length || 0} transactions
                    </List.Description>
                    <Button
                      primary
                      size="small"
                      style={{ marginTop: '0.5em' }}
                      onClick={() => openProof(s.receiptUrl)}
                    >
                      Download Proof
                    </Button>
                  </List.Content>
                </List.Item>
              ))}
            </List>
          )}

          {error && (
            <Message negative>
              <Message.Header>Error</Message.Header>
              <p>{error}</p>
            </Message>
          )}
        </Card.Content>
      </Card>
    </Grid.Column>
  )
}
