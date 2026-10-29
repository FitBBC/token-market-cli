# Token Market

Token Market connects a user's local Agent tools to models billed to a workspace.
These terms distinguish the user's desktop login from each tool's connection.

## Language

**Desktop Session**:
The user's reusable login to the Token Market desktop application, shared across tool-management operations and retained across application and computer restarts while valid.
_Avoid_: Agent API key, reusable authorization code

**Default Workspace**:
The user's platform-stored workspace preference shared by the browser and desktop application. Desktop workspace switching updates this preference.

**Current Workspace**:
The workspace selected in the desktop application for viewing balance, opening recharge, and establishing subsequent tool connections. Selecting it does not change existing connections.
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
The action that restores local configuration taken over by a Tool Connection while preserving later user changes. Server key disabling is best-effort and is not required for local disconnection to complete.
_Avoid_: Revoke, logout

**Reconnect**:
The user's explicit replacement of a tool's existing connection using the currently logged-in account and Current Workspace, including reloading its compatible model catalogue.
_Avoid_: Workspace switch, refresh of the old connection's catalogue

**Agent API Key**:
A model-access credential issued for a particular user's device, tool, and bound workspace. Other devices' connections and desktop logout do not revoke it.
_Avoid_: Desktop session

**CLI API Key**:
A model-access credential issued through the existing terminal CLI authorization flow for a particular tool and workspace.
_Avoid_: Desktop session

**Setup Session**:
A short-lived authorization attempt for the existing CLI setup flow, associated with a target tool.

**Authorization Code**:
A one-time result of the user's approval of a login or setup attempt, exchanged for the corresponding session or credential.
_Avoid_: PKCE code, desktop session

**Authorization Workspace**:
The workspace selected on the existing CLI setup consent page. Desktop login instead enters the default workspace; later desktop connections use the current workspace.

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

**Model Preset**:
A named model choice offered by the terminal CLI, such as recommended or fast.

**Adapter Directory Detection**:
The discovery of the local configuration location used by a particular tool installation.
