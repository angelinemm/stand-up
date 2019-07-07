# stand-up
Stand-up bot for slack

This is a simple/MVP stand-up bot for slack.

## Usage
1. Create a bot user on your Slack team, and get its api token
2. Setup the following env vars: 

| Key | Value |
| --- | --- |
| API_KEY | The key you got at step 1 |
| CHANNEL | The name of the slack channel you want the stand up to be posted on |
| TEAM_MEMBERS | The slack names (comma separated) of the team members who are taking part in the stand up |
| STAND_UP_TIME | The time you want the stand up to be posted ( for ex 9:30AM |
| NUMBER_OF_QUESTIONS | Number of questions/steps in your stand up |
| Q1 | Text of the first question |
| Qx | Text of each question |
