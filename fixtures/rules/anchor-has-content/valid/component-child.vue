<template>
  <!-- Nested text/menu components supply the accessible name. -->
  <NuxtLink to="/profile">
    <AccountInfo :account="account" />
  </NuxtLink>
  <NuxtLink to="/settings">
    <CommonDropdownItem text="Settings" icon="i-ri:settings-line" />
  </NuxtLink>
</template>
