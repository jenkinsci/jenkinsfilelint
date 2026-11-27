pipeline {
    agent any
    parameters { string(name: 'VERSION', defaultValue: '1') }
    stages {
        stage('S') {
            steps {
                sh "echo $CHANGE_TITLE ${params.VERSION} ${BUILD_NUMBER} ${env['CHANGE_ID']}"
            }
        }
    }
}
