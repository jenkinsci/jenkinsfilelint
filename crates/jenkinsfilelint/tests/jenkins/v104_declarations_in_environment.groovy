pipeline {
    agent any
    environment {
        def DNS = ''
        String REGION = 'eu'
        final ZONE = 'a'
        def A = 'a', B = 'b'
        COMMIT = sh(script: 'git rev-parse HEAD', , returnStdout: true).trim()
    }
    stages {
        stage('Build') {
            steps {
                echo "${DNS}${REGION}${ZONE}${A}${B}${COMMIT}"
            }
        }
    }
}
