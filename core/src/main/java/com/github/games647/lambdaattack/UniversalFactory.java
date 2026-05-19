package com.github.games647.lambdaattack;

import org.geysermc.mcprotocollib.network.Session;
import org.geysermc.mcprotocollib.protocol.packet.ingame.serverbound.ServerboundChatPacket;

import java.util.BitSet;

public class UniversalFactory {

    public static UniversalProtocol authenticate(GameVersion gameVersion, String username) {
        switch (gameVersion) {
            case VERSION_1_21_1:
                return new com.github.games647.lambdaattack.version.v1_21_1.ProtocolWrapper(username);
            default:
                throw new IllegalArgumentException("Invalid game version");
        }
    }

    public static void sendChatMessage(GameVersion gameVersion, String message, Session session) {
        switch (gameVersion) {
            case VERSION_1_21_1:
                session.send(new ServerboundChatPacket(
                        message,
                        System.currentTimeMillis(),
                        0L,
                        null,
                        0,
                        new BitSet()
                ));
                break;
            default:
                throw new IllegalArgumentException("Invalid game version");
        }
    }
}
