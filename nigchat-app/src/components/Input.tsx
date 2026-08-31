import React, { useState } from 'react';
import { StyleSheet, TextInput, TextInputProps, View, ViewStyle } from 'react-native';

import { radius, spacing, typography, useColors } from '../theme';
import { Icon, IconName } from './Icon';
import { Text } from './Text';

interface InputProps extends TextInputProps {
  label?: string;
  hint?: string;
  error?: string;
  icon?: IconName;
  right?: React.ReactNode;
  containerStyle?: ViewStyle;
}

export function Input({
  label,
  hint,
  error,
  icon,
  right,
  containerStyle,
  style,
  ...rest
}: InputProps) {
  const colors = useColors();
  const [focused, setFocused] = useState(false);

  // The focus ring is the brand green — one of the few places colour carries
  // meaning rather than decoration.
  const borderColor = error ? colors.danger : focused ? colors.primary : colors.border;

  return (
    <View style={containerStyle}>
      {label ? (
        <Text variant="subhead" tone="secondary" style={{ marginBottom: spacing.sm }}>
          {label}
        </Text>
      ) : null}

      <View
        style={[
          styles.field,
          {
            backgroundColor: colors.surfaceRaised,
            borderColor,
            borderWidth: focused || error ? 1.5 : 1,
          },
        ]}
      >
        {icon && <Icon name={icon} size={18} color={colors.textMuted} />}
        <TextInput
          style={[styles.input, { color: colors.text }, style]}
          placeholderTextColor={colors.textMuted}
          onFocus={(event) => {
            setFocused(true);
            rest.onFocus?.(event);
          }}
          onBlur={(event) => {
            setFocused(false);
            rest.onBlur?.(event);
          }}
          {...rest}
        />
        {right}
      </View>

      {(error || hint) && (
        <Text
          variant="footnote"
          tone={error ? 'danger' : 'muted'}
          style={{ marginTop: spacing.xs, marginLeft: spacing.xs }}
        >
          {error ?? hint}
        </Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  field: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    borderRadius: radius.md,
    paddingHorizontal: spacing.base,
    minHeight: 50,
  },
  input: { flex: 1, ...typography.body, paddingVertical: spacing.md },
});
