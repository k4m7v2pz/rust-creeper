package com.github.games647.lambdaattack.bot.listener;

import com.github.games647.lambdaattack.LambdaAttack;
import com.github.games647.lambdaattack.Options;
import com.github.games647.lambdaattack.bot.Bot;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.geysermc.mcprotocollib.network.event.session.DisconnectedEvent;
import org.geysermc.mcprotocollib.network.event.session.SessionAdapter;

import java.util.logging.Level;

public abstract class SessionListener extends SessionAdapter {

    protected final Options options;
    protected final Bot owner;

    public SessionListener(Options options, Bot owner) {
        this.options = options;
        this.owner = owner;
    }

    @Override
    public void disconnected(DisconnectedEvent event) {
        String reason = PlainTextComponentSerializer.plainText().serialize(event.getReason());
        owner.getLogger().log(Level.INFO, "Disconnected: {0}", reason);
    }

    public void onJoin() {
        if (options.autoRegister) {
            String password = LambdaAttack.PROJECT_NAME;
            owner.sendMessage(Bot.COMMAND_IDENTIFIER + "register " + password + ' ' + password);
            owner.sendMessage(Bot.COMMAND_IDENTIFIER + "login " + password);
        }
    }
}
