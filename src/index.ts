import { Server, Socket } from "socket.io";
import { User } from "./interfaces/user";
import { Ack, Command } from "./interfaces/Messages";
import RedisManager from "./redis";
import express from "express";
import { createServer } from "http";
import Packets from "./packets";

const app = express();
app.use(express.static(__dirname + "/public"));
const httpServer = createServer(app);
const io = new Server(httpServer);
const client = io.of("/ws_client");
const controller = io.of("/ws_controller");
const redis = new RedisManager(io);
const packets = new Packets();

client.on("connection", (socket) => {
  socket.on("publishUser", (user: User) => {
    redis.setSocketID(socket.id, user);
    console.log(user);
    socket.data.user = user;
    socket.send(packets.userLoggedIn());
  });
  socket.on("publishToControllers", (user: User) => {
    if (user.controllers != null) {
      user.controllers.forEach(async (_user) => {
        let _socketID = await redis.getSocket(_user);
        console.log(_socketID);
        if (_socketID != undefined || _socketID != null) {
          controller.sockets.get(_socketID)?.send("hello");
          socket.send(packets.userPublishedControllers());
        } else {
          socket.send(packets.noControllersOnline());
        }
      });
    }
  });
  socket.on("ackknowledgeCommandToClient", async (ack: Ack) => {
    let _socketID = await redis.getSocket(ack.issuer);
    if (_socketID != undefined || _socketID != null) {
      controller.sockets.get(_socketID)?.send(packets.userReceivedCommand());
    } else {
      socket.send(packets.noControllersOnline());
    }
  });
  socket.on("disconnect", () => {
    console.log("Disconnect Client")
    redis.deleteKey(socket.data.user);
  });
});
controller.on("connection", (socket: Socket) => {
  socket.on("publishUser", (user: User) => {
    console.log(user);
    redis.setSocketID(socket.id, user);
    socket.data.user = user;
    socket.send(packets.userLoggedIn());
  });
  socket.on("publishToClients", (user: User) => {
    if (user.clients != undefined) {
      user.clients.forEach(async (_user) => {
        let _socketID = await redis.getSocket(_user);
        if (_socketID != undefined || _socketID != null) {
          client.sockets.get(_socketID)?.send("hello");
          socket.send(packets.userPublishedClients());
        } else {
          redis.deleteKey(user);
          socket.send(packets.noClientsOnline());
        }
      });
    }
  });
  socket.on("command", async (command: Command) => {
    console.log(command);
    let _socketID = await redis.getSocket(command.receiver);
    if (_socketID != undefined || _socketID != null) {
      client.sockets.get(_socketID)?.emit("command", command);
      socket.send(packets.userSentCommand());
    } else {
      redis.deleteKey(command.receiver);
      socket.send(packets.noClientsOnline());
    }
  });
  socket.on("disconnect", () => {
    console.log("Disconnect Controller")
    redis.deleteKey(socket.data.user);
  });
});
httpServer.listen(3000);
