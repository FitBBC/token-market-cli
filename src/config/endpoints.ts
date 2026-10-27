export function consoleApiBaseUrl(): string {
  return process.env.TOKEN_MARKET_CONSOLE_API_BASE_URL ?? 'https://test.tokensapi.cn/console';
}

export function gatewayApiBaseUrl(): string {
  return process.env.TOKEN_MARKET_GATEWAY_API_BASE_URL ?? 'https://api-test.tokensapi.cn';
}
