export interface ModelPreset {
  id: string;
  displayName: string;
  model: string;
}

export const modelPresets: ModelPreset[] = [
  { id: 'recommended', displayName: 'GLM 5.2', model: 'glm-5.2' },
  { id: 'fast', displayName: 'DeepSeek V4.1 Flash', model: 'deepseek-v4.1-flash' },
];

export function resolveModel(presetId?: string): string {
  return modelPresets.find((preset) => preset.id === presetId)?.model ?? modelPresets[0].model;
}
