package com.example.peitch;

import com.intellij.openapi.actionSystem.AnAction;
import com.intellij.openapi.actionSystem.AnActionEvent;
import com.intellij.openapi.ui.Messages;

public class HelloAction extends AnAction {
    
    @Override
    public void actionPerformed(AnActionEvent e) {
        Messages.showMessageDialog(
            e.getProject(),
            "Hello from Peitch Plugin!",
            "Peitch",
            Messages.getInformationIcon()
        );
    }
}