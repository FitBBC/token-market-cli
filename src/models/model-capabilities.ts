export type ReasoningEffort = 'low' | 'medium' | 'high' | 'xhigh' | 'max';

export interface ModelCapabilities {
  reasoning: {
    defaultEffort: ReasoningEffort;
    supportedEfforts: ReasoningEffort[];
    canDisableThinking: boolean;
  };
}

const modelCapabilities: Record<string, ModelCapabilities> = {
  'glm-5.2': {
    reasoning: {
      defaultEffort: 'max',
      supportedEfforts: ['high', 'max'],
      canDisableThinking: true,
    },
  },
  'glm-5.3': {
    reasoning: {
      defaultEffort: 'max',
      supportedEfforts: ['low', 'high', 'max'],
      canDisableThinking: false,
    },
  },
};

export function resolveModelCapabilities(model: string): ModelCapabilities {
  const capabilities = modelCapabilities[model];
  if (!capabilities) throw new Error(`unsupported Token Market model: ${model}`);
  return capabilities;
}
