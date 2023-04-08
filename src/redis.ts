import { RedisClientType, createClient } from 'redis';
import { Server, Socket } from 'socket.io';
import { User } from './interfaces/user';
export default class RedisManager{
    _redis: RedisClientType;
    io: Server;
    /**
     *  Makes a redis client and setup everything else
     */
    constructor(socketIOClient: Server) {
        this.io = socketIOClient;
        this._redis = createClient();
        this._redis.connect().then(()=>console.log(this._redis));
    }
    async getSocket(user: User): Promise<string | undefined> {
        let socketID = await this._redis.get(user.name).catch(err => console.log(err));
        console.log(socketID)
        if (socketID != undefined){
            return socketID
        }
        else{
            return undefined
        }
        return undefined
    }
    async setSocketID(socketID: string, user: User){
        await this._redis.set(user.name.toString(), socketID.toString()).catch(err => console.log(err))
    }
    async deleteKey(user: User){
        this._redis.del(user.name.toString())
    }
}