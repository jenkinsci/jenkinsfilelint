pipeline {
    agent any
    stages {
        stage('W') {
            when {
                not { branch 'main' }
                changeRequest()
                changeset '**/*.js'
                buildingTag()
                tag 'v*'
                triggeredBy 'TimerTrigger'
                equals expected: 2, actual: currentBuild.number
            }
            steps { echo 'w' }
        }
    }
}
