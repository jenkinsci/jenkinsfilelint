pipeline {
    agent any
    stages {
        stage('Notify') {
            steps {
                sh "echo Building ${env.CHANGE_TITLE}"
                sh "git checkout $CHANGE_BRANCH"
                sh """
                    curl -d "author=${CHANGE_AUTHOR_DISPLAY_NAME}" https://example.com/notify
                """
            }
        }
    }
}
