export function consoleApiBaseUrl(): string {
  return process.env.TOKEN_MARKET_CONSOLE_API_BASE_URL ?? 'https://console-api.tokensapi.cn';
}

export function gatewayApiBaseUrl(): string {
  return process.env.TOKEN_MARKET_GATEWAY_API_BASE_URL ?? 'https://api.tokensapi.cn';
}
