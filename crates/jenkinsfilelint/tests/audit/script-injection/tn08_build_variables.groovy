pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                sh "make BUILD=${env.BUILD_NUMBER} JOB=${env.JOB_NAME} PR=${env.CHANGE_ID}"
                sh "cd ${env.WORKSPACE} && ./build.sh ${env.GIT_COMMIT}"
            }
        }
    }
}
