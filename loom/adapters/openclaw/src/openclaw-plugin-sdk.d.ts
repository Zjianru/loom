declare module "openclaw/plugin-sdk" {
  export type OpenClawConfig = Record<string, unknown>;

  export type OpenClawPluginToolContext = {
    config?: OpenClawConfig;
    workspaceDir?: string;
    agentDir?: string;
    agentId?: string;
    sessionKey?: string;
    requesterSessionKey?: string;
    // Ephemeral session UUID regenerated on /new and /reset.
    sessionId?: string;
    messageChannel?: string;
    agentAccountId?: string;
    requesterSenderId?: string;
    senderIsOwner?: boolean;
    sandboxed?: boolean;
    threadId?: string;
    conversationId?: string;
    runId?: string;
    toolCallId?: string;
    channelId?: string;
    accountId?: string;
    [key: string]: unknown;
  };

  export type OpenClawPluginLogger = {
    debug?: (message: string, meta?: unknown) => void;
    info?: (message: string, meta?: unknown) => void;
    warn?: (message: string, meta?: unknown) => void;
    error?: (message: string, meta?: unknown) => void;
  };

  export type OpenClawPluginHookPayload = OpenClawPluginToolContext & {
    prompt?: unknown;
    messages?: unknown[];
    message?: unknown;
    content?: unknown;
    metadata?: unknown;
    timestamp?: unknown;
    toolName?: unknown;
    params?: unknown;
    childSessionKey?: unknown;
    targetSessionKey?: unknown;
    reason?: unknown;
    outcome?: unknown;
    error?: unknown;
    endedAt?: unknown;
  };

  export type OpenClawPluginToolDescriptor = {
    name?: string;
    label?: string;
    description?: string;
    parameters?: Record<string, unknown>;
    execute?: (toolCallId: string, params: unknown) => unknown | Promise<unknown>;
    [key: string]: unknown;
  };

  export type AnyAgentTool = OpenClawPluginToolDescriptor;
  export type OpenClawPluginToolFactoryResult = AnyAgentTool | AnyAgentTool[] | null | undefined;
  export type OpenClawPluginToolFactory = (
    ctx: OpenClawPluginToolContext,
  ) => OpenClawPluginToolFactoryResult;
  export type OpenClawPluginToolOptions = {
    name?: string;
    names?: string[];
    optional?: boolean;
    [key: string]: unknown;
  };

  export type PluginCommandContext = {
    senderId?: string;
    channel: string;
    channelId?: string;
    isAuthorizedSender: boolean;
    args?: string;
    commandBody: string;
    config: OpenClawConfig;
    from?: string;
    to?: string;
    accountId?: string;
    messageThreadId?: number;
    [key: string]: unknown;
  };

  export type PluginCommandResult = {
    text?: string;
    [key: string]: unknown;
  };

  export type OpenClawPluginCommandDefinition = {
    name: string;
    nativeNames?: Partial<Record<string, string>> & {
      default?: string;
    };
    description: string;
    acceptsArgs?: boolean;
    requireAuth?: boolean;
    handler: (ctx: PluginCommandContext) => PluginCommandResult | Promise<PluginCommandResult>;
  };

  export type OpenClawPluginHttpRouteAuth = "gateway" | "plugin";
  export type OpenClawPluginHttpRouteMatch = "exact" | "prefix";
  export type OpenClawPluginHttpRouteHandler = (
    req: unknown,
    res: unknown,
  ) => boolean | void | Promise<boolean | void>;
  export type OpenClawPluginHttpRouteParams = {
    path: string;
    handler: OpenClawPluginHttpRouteHandler;
    auth: OpenClawPluginHttpRouteAuth;
    match?: OpenClawPluginHttpRouteMatch;
    replaceExisting?: boolean;
  };

  export type OpenClawPluginCliContext = {
    program: unknown;
    config: OpenClawConfig;
    workspaceDir?: string;
    logger: OpenClawPluginLogger;
  };

  export type OpenClawPluginCliRegistrar = (
    ctx: OpenClawPluginCliContext,
  ) => unknown | Promise<unknown>;

  export type OpenClawPluginServiceContext = {
    config: OpenClawConfig;
    workspaceDir?: string;
    stateDir: string;
    logger: OpenClawPluginLogger;
  };

  export type OpenClawPluginService = {
    id: string;
    start?: (ctx?: OpenClawPluginServiceContext) => unknown | Promise<unknown>;
    stop?: (ctx?: OpenClawPluginServiceContext) => unknown | Promise<unknown>;
    [key: string]: unknown;
  };

  export type OpenClawPluginChannelRegistration =
    | {
        plugin: Record<string, unknown>;
        dock?: unknown;
      }
    | Record<string, unknown>;

  export type ProviderPlugin = Record<string, unknown>;
  export type OpenClawPluginHookOptions = {
    entry?: unknown;
    name?: string;
    description?: string;
    register?: boolean;
    [key: string]: unknown;
  };

  export type OpenClawPluginApi = {
    id?: string;
    name?: string;
    version?: string;
    description?: string;
    source?: string;
    config?: OpenClawConfig;
    pluginConfig?: Record<string, unknown>;
    logger: OpenClawPluginLogger;
    runtime?: {
      system?: {
        enqueueSystemEvent: (text: string, meta?: Record<string, unknown>) => void;
        runCommandWithTimeout?: (
          argv: string[],
          options: { timeoutMs: number; cwd?: string; env?: NodeJS.ProcessEnv },
        ) => Promise<{
          stdout: string;
          stderr: string;
          code: number | null;
          signal: NodeJS.Signals | null;
          killed: boolean;
          termination: "exit" | "timeout" | "no-output-timeout" | "signal";
          noOutputTimedOut?: boolean;
        }>;
      };
      channel?: {
        routing?: {
          buildAgentSessionKey?: (params: Record<string, unknown>) => string;
          resolveAgentRoute?: (params: Record<string, unknown>) => Record<string, unknown>;
        };
      };
      [key: string]: unknown;
    };
    resolvePath: (path: string) => string;
    on: (
      eventName: string,
      handler: (event: OpenClawPluginHookPayload, ctx: OpenClawPluginHookPayload) => unknown,
      opts?: { priority?: number },
    ) => void;
    registerHook?: (
      events: string | string[],
      handler: (event: OpenClawPluginHookPayload, ctx: OpenClawPluginHookPayload) => unknown,
      opts?: OpenClawPluginHookOptions,
    ) => void;
    registerTool: (
      tool: AnyAgentTool | OpenClawPluginToolFactory,
      options?: OpenClawPluginToolOptions,
    ) => void;
    registerHttpRoute?: (params: OpenClawPluginHttpRouteParams) => void;
    registerChannel?: (registration: OpenClawPluginChannelRegistration) => void;
    registerGatewayMethod?: (method: string, handler: (...args: unknown[]) => unknown) => void;
    registerCli?: (
      registrar: OpenClawPluginCliRegistrar,
      options?: { commands?: string[]; [key: string]: unknown },
    ) => void;
    registerService: (service: OpenClawPluginService) => void;
    registerProvider?: (provider: ProviderPlugin) => void;
    registerCommand?: (command: OpenClawPluginCommandDefinition) => void;
    registerContextEngine?: (id: string, factory: (...args: unknown[]) => unknown) => void;
    getConfig?: <T = unknown>(path: string) => T | undefined;
    [key: string]: unknown;
  };
}
