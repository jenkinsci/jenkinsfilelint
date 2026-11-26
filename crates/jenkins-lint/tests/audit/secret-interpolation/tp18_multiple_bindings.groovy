pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([
                    string(credentialsId: 'a', variable: 'A_TOKEN'),
                    usernameColonPassword(credentialsId: 'b', variable: 'B_AUTH'),
                    certificate(credentialsId: 'c', keystoreVariable: 'STORE', passwordVariable: 'STORE_PASS', aliasVariable: 'ALIAS'),
                ]) {
                    sh "a ${A_TOKEN}; b ${B_AUTH}; c ${STORE} ${STORE_PASS} ${ALIAS}"
                }
            }
        }
    }
}
