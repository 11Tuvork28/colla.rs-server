export default class Packets{
    userLoggedIn(): Record<string, string>{
        return {
            message: "User was logged in successfully",
            action: "publish"
        }
    }
    userPublishedClients(): Record<string, string>{
        return {
            message: "User published clients successfully",
            action: "ready" 
        }
    }
    userPublishedControllers(): Record<string, string>{
        return {
            message: "User published controllers successfully",
            action: "ready"
        }
    }
    userSentCommand(): Record<string, string>{
        return {
            message: "User send command successfully",
            action: "awaitAck"
        }
    }
    userReceivedCommand(): Record<string, string>{
        return {
            message: "User received command successfully",
            action: "sendAck"
        }
    }
    noClientsOnline(): Record<string, string>{
        return {
            message: "No clients online",
            action: "none"
        }
    }
    noControllersOnline(): Record<string, string>{
        return {
            message: "No controllers online",
            action: "none"
        }
    }
}