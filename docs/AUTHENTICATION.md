# Local inference authentication

Authentication is opt-in. Configure the local API base first, then run
`cogfabric configure-auth --enabled true`. Supply the operator's bearer token
through `COGFABRIC_INFERENCE_API_KEY` in the launching environment. Do not put
credentials in command arguments, configuration, model manifests or chat messages.
Disable authentication using `configure-auth --enabled false`.

Configuration stores only the approved local API base. Requests attach the token
only when their canonical endpoint matches that scope. Changing to another endpoint,
including a registered expert endpoint, does not forward the original token.
Missing or invalid dedicated credentials fail matching authenticated requests.
Discovery uses the same scope rules. Redirects, proxies and remote endpoints remain
disallowed. The operator must explicitly configure a new scope after changing APIs.

Managed workers receive only the dedicated credential in their cleared environment;
typed bootstrap carries the optional scope, never the token. Older bootstrap packets
without this appended field remain unauthenticated. This does not protect against
host administrators or malicious processes running as the same user.

Raw and JSON-decoded provider responses are checked for exact credential echoes
before results enter task traces. Invalid JSON errors do not include provider text.
This is defense in depth, not protection against every transformation of a secret
by a malicious authenticated provider. Credentials are not zeroized in memory.

Synthetic integration tests cover direct and managed generation and escaped echoes.
They do not prove live LM Studio authentication or reasoning quality. Real generation
remains unverified until an operator supplies the server's credential locally.

Additional CLI tests cover primary and registered discovery with matching and
nonmatching scopes, escaped echoes, and missing credentials before any connection
for direct and managed tasks. These fixtures use synthetic child environments and
do not inspect the operator's credentials.
