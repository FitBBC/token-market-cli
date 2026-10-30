# Token Market

Token Market connects a user's local Agent tools to models billed to a workspace.
These terms distinguish the user's client login from each tool's connection.

## Language

**Token Market Client**:
The local application used to manage Agent tools, through either the terminal CLI or desktop interface. These are two interfaces to the same client identity and tool connections on a device.
_Avoid_: Agent tool

**Client Session**:
The user's reusable login shared by the Token Market CLI and desktop interface on a device. Ending it does not end existing Tool Connections or revoke their Agent API Keys.
_Avoid_: Agent API key, authorization code

**Default Workspace**:
The user's platform-stored workspace preference shared by the browser and Token Market Client.

**Current Workspace**:
The workspace selected in the Token Market Client for viewing balance, opening recharge, and establishing subsequent tool connections. The CLI and desktop share this selection; selecting it does not change existing connections.
_Avoid_: Bound workspace

**Bound Workspace**:
The workspace to which a tool connection's API key belongs and against which its model usage is billed.
_Avoid_: Current workspace

**Tool Connection**:
A local Agent tool's configured access to Token Market through an Agent API Key belonging to its bound workspace. This release manages one active configuration per tool on a device; its connection is independent of desktop login and other devices.
_Avoid_: Desktop login

**Managed Configuration File**:
A local Agent configuration or credential file modified by a Tool Connection. Its current contents may also include settings maintained by the user or the Agent itself.

**Disconnect**:
The action that restores local configuration taken over by a Tool Connection while preserving later user changes. The Agent API Key remains available for future connections.
_Avoid_: Revoke, logout

**Reconnect**:
The user's explicit replacement of a tool's existing connection using the Token Market Client's currently logged-in account and Current Workspace, including reloading its compatible model catalogue and reusing that account and workspace’s saved Agent API Key when available.
_Avoid_: Workspace switch, refresh of the old connection's catalogue

**Agent API Key**:
A model-access credential issued for a particular user's device, tool, and bound workspace. The CLI and desktop reuse the same credential; workspace switches, disconnection, other devices, and client logout do not revoke it.
_Avoid_: Client Session, CLI-specific key, desktop-specific key

**Key Fingerprint**:
An irreversible identifier of a complete Agent API Key, used to determine whether a locally saved credential matches the platform's current credential.
_Avoid_: Encrypted API key, masked key

**Key Reset**:
The platform operation that replaces an API key's secret while preserving its key record and identity.
_Avoid_: Reconnect, disconnect, key deletion

**Client Login Attempt**:
A short-lived attempt to authorize a reusable Client Session through the browser, independently of connecting any Agent Tool.
_Avoid_: Tool connection, Key issuance

**Client Key Issuance**:
A tracked attempt to provide a new Agent API Key to one client-managed tool scope, ending when the client confirms that its credential has been saved.
_Avoid_: Client login, successful tool configuration

**Issuance Confirmation**:
The client's acknowledgement that it has safely saved a particular issued credential. It does not state that the tool has been configured or tested.

**Authorization Code**:
A one-time result of the user's approval of a login or setup attempt, exchanged for the corresponding session or credential.
_Avoid_: PKCE code, desktop session

**Model Catalogue**:
The models visible to a particular API key together with their advertised capabilities, including supported request endpoints.

**Connection Test**:
A user-triggered check of a tool's existing connection using its configured key and request protocol. A multi-model test uses one model explicitly selected for that test and leaves the tool's configuration unchanged.

**Tool Adapter**:
The Token Market integration responsible for connecting one supported Agent product.

**Revoke**:
The action that disables an API key while preserving its audit record.
_Avoid_: Delete key, logout

**Source**:
The origin of an API key, distinguishing credentials issued through client authorization from keys created in the console.

**Tool**:
The target Agent product, such as Claude Code, Codex CLI, WorkBuddy, DeepSeek Harness, OpenClaw, or OpenCode.

**Adapter Directory Detection**:
The discovery of the local configuration location used by a particular tool installation.
