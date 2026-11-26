def utils = [:]

@NonCPS
def parseJson(String text) {
    return new groovy.json.JsonSlurperClassic().parseText(text)
}

class Version implements Comparable<Version> {
    int major, minor
    Version(String s) {
        def parts = s.tokenize('.')
        major = parts[0] as int
        minor = parts.size() > 1 ? parts[1] as int : 0
    }
    int compareTo(Version o) { major <=> o.major ?: minor <=> o.minor }
    String toString() { "${major}.${minor}" }
}

pipeline {
    agent any
    environment {
        PATH = "/opt/bin:${env.PATH}"
    }
    stages {
        stage('Groovy') {
            steps {
                script {
                    def list = [1, 2, 3].collect { it * 2 }
                    def map = [a: 1, 'b-c': 2, (list[0]): 'x']
                    def total = list.sum()
                    def name = env.BRANCH_NAME ?: 'unknown'
                    def len = env.CHANGE_ID?.length()
                    def files = findFiles(glob: '**/*.txt')*.path
                    def re = /^v(\d+)\.(\d+)$/
                    def m = ('v1.2' =~ re)
                    boolean ok = 'v1.2' ==~ re
                    for (int i = 0; i < 3; i++) {
                        echo "i=${i}"
                    }
                    for (f in files) { echo f }
                    (0..<2).each { idx -> echo "idx ${idx}" }
                    switch (name) {
                        case 'main':
                            echo 'main'
                            break
                        case ~/release-.*/:
                            echo 'release'
                            break
                        default:
                            echo 'other'
                    }
                    try {
                        sh 'exit 1'
                    } catch (Exception e) {
                        echo "caught ${e.message}"
                    } finally {
                        echo 'finally'
                    }
                    def multi = """
                        line one ${name}
                        line two
                    """.stripIndent()
                    def single = '''no ${interp} here'''
                    def dollarSlashy = $/path\to\${name}/$
                    def result = sh(
                        script: 'echo done',
                        returnStdout: true
                    ).trim()
                    def chained = [3, 1, 2]
                        .sort()
                        .reverse()
                    assert chained.size() == 3 : 'size mismatch'
                    if (total instanceof Integer && !(total in [0, 1])) {
                        currentBuild.description = "total=${total}"
                    }
                    withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'U', passwordVariable: 'P')]) {
                        sh 'echo $U'
                    }
                    dir('sub') {
                        writeFile file: 'a.txt', text: "${map.a}"
                    }
                    def closure = { String a, int b = 2 -> "${a}${b}" }
                    echo closure('x')
                    echo new Version('1.2').toString()
                    utils.greet = { n -> "hi ${n}" }
                    echo utils.greet('bob')
                }
            }
        }
    }
}
