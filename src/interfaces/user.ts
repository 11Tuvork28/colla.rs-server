export enum UserTypes{
    Client,
    Controller
}

export interface User {
    name: string,
    controllers?: Array<User> | null,
    clients?: Array<User> | null,
    type: UserTypes
}