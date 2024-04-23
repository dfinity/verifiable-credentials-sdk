module.exports = {
  testEnvironment: "jsdom",
  transform: {
    "^.+\\.(t|j)s$": [
      "ts-jest",
      {
        tsconfig: {
          allowJs: true,
        },
      },
    ],
  },
};
