import { User } from "./user";

export enum Modes {
  Zappy = 4,
  Vibey = 3,
  Beep = 2,
  Led = 1,
}

export interface Command {
  mode: Modes;
  level: number;
  duration: number;
  receiver: User;
  issuer: User;
}
export interface Ack {
  command: Command;
  issuer: User;
  acked: boolean;
}
