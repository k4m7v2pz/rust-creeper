package com.github.games647.lambdaattack.bot.listener;

import com.github.games647.lambdaattack.Options;
import com.github.games647.lambdaattack.bot.Bot;
import com.github.games647.lambdaattack.bot.EntitiyLocation;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.geysermc.mcprotocollib.network.Session;
import org.geysermc.mcprotocollib.network.packet.Packet;
import org.geysermc.mcprotocollib.protocol.packet.ingame.clientbound.ClientboundLoginPacket;
import org.geysermc.mcprotocollib.protocol.packet.ingame.clientbound.ClientboundSystemChatPacket;
import org.geysermc.mcprotocollib.protocol.packet.ingame.clientbound.entity.player.ClientboundPlayerPositionPacket;
import org.geysermc.mcprotocollib.protocol.packet.ingame.clientbound.entity.player.ClientboundSetHealthPacket;

import java.util.logging.Level;

public class SessionListener1211 extends SessionListener {

    public SessionListener1211(Options options, Bot owner) {
        super(options, owner);
    }

    @Override
    public void packetReceived(Session session, Packet packet) {
        if (packet instanceof ClientboundSystemChatPacket) {
            ClientboundSystemChatPacket chatPacket = (ClientboundSystemChatPacket) packet;
            String message = PlainTextComponentSerializer.plainText().serialize(chatPacket.getContent());
            owner.getLogger().log(Level.INFO, "Received Message: {0}", message);
        } else if (packet instanceof ClientboundPlayerPositionPacket) {
            ClientboundPlayerPositionPacket posPacket = (ClientboundPlayerPositionPacket) packet;
            double posX = posPacket.getX();
            double posY = posPacket.getY();
            double posZ = posPacket.getZ();
            float pitch = posPacket.getPitch();
            float yaw = posPacket.getYaw();
            EntitiyLocation location = new EntitiyLocation(posX, posY, posZ, pitch, yaw);
            owner.setLocation(location);
        } else if (packet instanceof ClientboundSetHealthPacket) {
            ClientboundSetHealthPacket healthPacket = (ClientboundSetHealthPacket) packet;
            owner.setHealth(healthPacket.getHealth());
            owner.setFood(healthPacket.getFood());
        } else if (packet instanceof ClientboundLoginPacket) {
            super.onJoin();
        }
    }
}
