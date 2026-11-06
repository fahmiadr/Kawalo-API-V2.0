import { EnvConfig } from '@core/config/env.config';
import { createPinoLogger } from '@bogeychan/elysia-logger';
import pretty from 'pino-pretty';

const prettyStream = pretty({
  colorize: true,
  translateTime: 'SYS:HH:MM:ss',
  ignore: 'pid,hostname,reqMsg',
  messageFormat: (log, key) => (log.reqMsg as string) || (log[key] as string) || '',
});

export const logger = createPinoLogger({
  stream: prettyStream,
  level: EnvConfig.LOG_LEVEL,
});

export type AppLogger = typeof logger;
