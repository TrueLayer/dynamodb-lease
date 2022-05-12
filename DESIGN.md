# dynamodb-lease design
_dynanmodb-lease_ makes use of dynamodb condition experssions & ttl to provide concurrent-safe distributed leases.

## Table
The lease table has the following fields:

* `key` (S, hash key)
* `lease_expiry` (N, ttl enabled)
* `lease_version` (S)

## Acquire, extend, drop algorithm
To acquire a lease for key `foo` _(using default config values)_
* _PutItem_ with key: `foo` with:
  - `lease_version` a unique id.
  - `lease_expiry` unix timestamp set to 60s from now.
  - Condition that the item does not exist yet.
* In the background periodically _UpdateItem_ key: `foo` with:
  - `lease_version` a new unique id.
  - `lease_expiry` 60s from now.
  - Condition that the `lease_version` is the previous value.

The lease is now alive an cannot be acquired elsewhere.

When finished the `Lease` is dropped.
* On drop _DeleteItem_ key `foo`
  - Condition that the `lease_version` is the current value.

A new lease can now be acquired.

## Edge cases, issues & error scenarios
### Lost access to db after acquiring lease
If access to the db is lost _after_ acquiring a lease the background task will be unable to _UpdateItem_ to extend the lease. The lease also will not be able to _DeleteItem_ on drop.

* The lease is still exclusive for the original `lease_expiry` ttl. 
  It makes sense then to set the ttl to longer than the expected max duration needed to provide a decent guarantee of exclusivity.
* As _DeleteItem_ fails other tasks will remain blocked, but only until the `lease_expiry` ttl triggers dynamodb to remove the item. So this is not a deadlock, but does inform that the ttl shouldn't be _too_ long.

### Clock skew
The client uses the local clock to generate `lease_expiry` timestamps. To mitigate client clock skews consider lengthening the `lease_expiry` ttl.
