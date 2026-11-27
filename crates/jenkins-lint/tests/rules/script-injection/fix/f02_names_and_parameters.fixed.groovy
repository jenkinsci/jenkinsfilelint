pipeline {
    agent any
    parameters { string(name: 'VERSION', defaultValue: '1') }
    stages {
        stage('S') {
            steps {
                sh 'echo "$CHANGE_TITLE" "$VERSION" "$BUILD_NUMBER" "$CHANGE_ID"'
            }
        }
    }
}
